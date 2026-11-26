using System;
using System.Collections.Concurrent;
using System.Collections.Generic;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Text;
using System.Text.RegularExpressions;
using System.Threading;
using System.Windows.Forms;

sealed class StreamSmashWindow : Form
{
    static readonly string Engine = Path.Combine(AppDomain.CurrentDomain.BaseDirectory, "stream-smash.exe");
    readonly ComboBox mode = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList };
    readonly TextBox source = new TextBox();
    readonly TextBox audio = new TextBox();
    readonly TextBox resolution = new TextBox { Text = "1920x1080" };
    readonly NumericUpDown fps = new NumericUpDown { Minimum = 1, Maximum = 120, Value = 60 };
    readonly TextBox folder = new TextBox { Text = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyVideos), "Stream Smash") };
    readonly TextBox rtmp = new TextBox { UseSystemPasswordChar = true };
    readonly Button browse = new Button { Text = "Browse media", AutoSize = true };
    readonly Button devices = new Button { Text = "List devices", AutoSize = true };
    readonly Button start = new Button { Text = "Start recording", AutoSize = true };
    readonly Button stop = new Button { Text = "Stop", AutoSize = true };
    readonly Label status = new Label { Text = "Ready", AutoSize = true, Margin = new Padding(3, 5, 3, 5) };
    readonly TextBox activity = new TextBox { Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Both, WordWrap = false, Dock = DockStyle.Fill };
    readonly System.Windows.Forms.Timer timer = new System.Windows.Forms.Timer { Interval = 100 };
    readonly ConcurrentQueue<string> messages = new ConcurrentQueue<string>();
    Process engine;
    int closedPipes;
    bool recording, stopping, closing;

    [STAThread]
    static void Main()
    {
        Application.EnableVisualStyles();
        Application.SetCompatibleTextRenderingDefault(false);
        using (var window = new StreamSmashWindow()) Application.Run(window);
    }

    StreamSmashWindow()
    {
        Text = "Stream Smash";
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(760, 540);
        MinimumSize = new Size(620, 490);
        Font = new Font("Segoe UI", 10);
        var layout = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(16), ColumnCount = 2, RowCount = 10 };
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 161));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        for (int row = 0; row < 9; row++) layout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        Controls.Add(layout);
        mode.Items.AddRange(new object[] { "Test signal", "Windows capture device", "Media file" });
        mode.SelectedIndex = 0;
        AddField(layout, 0, "Input", mode);
        AddField(layout, 1, "Device name / file", source);
        AddField(layout, 2, "Audio device", audio);
        AddField(layout, 3, "Output resolution", resolution);
        AddField(layout, 4, "Frames per second", fps);
        AddField(layout, 5, "Recording folder", folder);
        AddField(layout, 6, "RTMP(S) URL (optional)", rtmp);
        var actions = new FlowLayoutPanel { AutoSize = true, Dock = DockStyle.Fill };
        actions.Controls.AddRange(new Control[] { devices, browse, start, stop });
        AddWide(layout, 7, actions);
        AddWide(layout, 8, status);
        AddWide(layout, 9, activity);
        mode.SelectedIndexChanged += (sender, args) => UpdateControls();
        rtmp.TextChanged += (sender, args) => start.Text = string.IsNullOrWhiteSpace(rtmp.Text) ? "Start recording" : "Record and stream";
        devices.Click += (sender, args) => Launch(new[] { "devices" }, false, "");
        browse.Click += (sender, args) => {
            using (var dialog = new OpenFileDialog())
                if (dialog.ShowDialog(this) == DialogResult.OK) source.Text = dialog.FileName;
        };
        start.Click += (sender, args) => StartRecording();
        stop.Click += (sender, args) => StopRecording();
        FormClosing += (sender, args) => {
            if (engine == null || !recording || engine.HasExited) return;
            args.Cancel = true;
            closing = true;
            StopRecording();
        };
        FormClosed += (sender, args) => timer.Dispose();
        timer.Tick += (sender, args) => ReadActivity();
        UpdateControls();
        timer.Start();
    }

    static void AddField(TableLayoutPanel table, int row, string label, Control field)
    {
        table.Controls.Add(new Label { Text = label, TextAlign = ContentAlignment.MiddleLeft, Dock = DockStyle.Fill }, 0, row);
        field.Dock = DockStyle.Fill;
        table.Controls.Add(field, 1, row);
    }

    static void AddWide(TableLayoutPanel table, int row, Control control)
    {
        table.Controls.Add(control, 0, row);
        table.SetColumnSpan(control, 2);
    }

    void UpdateControls()
    {
        bool idle = engine == null;
        foreach (var control in new Control[] { mode, resolution, fps, folder, rtmp, devices, start }) control.Enabled = idle;
        source.Enabled = idle && mode.SelectedIndex != 0;
        audio.Enabled = idle && mode.SelectedIndex == 1;
        browse.Enabled = idle && mode.SelectedIndex == 2;
        stop.Enabled = !idle && recording && !stopping;
    }

    void StartRecording()
    {
        if ((mode.SelectedIndex != 0 && string.IsNullOrWhiteSpace(source.Text))
            || string.IsNullOrWhiteSpace(folder.Text) || string.IsNullOrWhiteSpace(resolution.Text)) {
            status.Text = "Enter an input, resolution and recording folder.";
            return;
        }
        var args = new List<string> { "record", "--size", resolution.Text.Trim(), "--fps", fps.Value.ToString(), "--output-dir", folder.Text.Trim() };
        switch (mode.SelectedIndex) {
            case 0: args.Add("--test-pattern"); break;
            case 1:
                args.AddRange(new[] { "--video-device", source.Text.Trim() });
                if (!string.IsNullOrWhiteSpace(audio.Text)) args.AddRange(new[] { "--audio-device", audio.Text.Trim() });
                break;
            case 2: args.AddRange(new[] { "--input", source.Text.Trim() }); break;
        }
        string destination = rtmp.Text.Trim();
        if (destination.Length > 0) args.Add("--stream");
        Launch(args, true, destination);
    }

    static string Quote(string argument)
    {
        string escaped = Regex.Replace(argument, @"(\\*)""", "$1$1\\\"");
        return "\"" + Regex.Replace(escaped, @"(\\+)$", "$1$1") + "\"";
    }

    void Launch(IEnumerable<string> args, bool record, string destination)
    {
        var info = new ProcessStartInfo {
            FileName = Engine, Arguments = string.Join(" ", args.Select(Quote)), UseShellExecute = false,
            CreateNoWindow = true, RedirectStandardOutput = true, RedirectStandardError = true,
            RedirectStandardInput = true, StandardOutputEncoding = Encoding.UTF8, StandardErrorEncoding = Encoding.UTF8
        };
        if (destination.Length > 0) info.EnvironmentVariables["STREAM_SMASH_RTMP_URL"] = destination;
        else info.EnvironmentVariables.Remove("STREAM_SMASH_RTMP_URL");
        var child = new Process { StartInfo = info };
        child.OutputDataReceived += ReceiveLine;
        child.ErrorDataReceived += ReceiveLine;
        try { child.Start(); }
        catch (System.ComponentModel.Win32Exception error) {
            child.Dispose();
            status.Text = "Could not start the Rust engine: " + error.Message;
            return;
        }
        activity.Clear();
        closedPipes = 0;
        engine = child;
        recording = record;
        stopping = false;
        status.Text = "Starting...";
        UpdateControls();
        child.BeginOutputReadLine();
        child.BeginErrorReadLine();
    }

    void ReceiveLine(object sender, DataReceivedEventArgs args)
    {
        if (args.Data == null) Interlocked.Increment(ref closedPipes);
        else messages.Enqueue(args.Data);
    }

    void StopRecording()
    {
        if (stopping || engine == null || engine.HasExited) return;
        try {
            engine.StandardInput.WriteLine("q");
            engine.StandardInput.Flush();
            stopping = true;
            status.Text = "Finalizing recording...";
            UpdateControls();
        } catch (IOException error) { status.Text = error.Message; }
    }

    void ReadActivity()
    {
        if (engine == null) return;
        string line;
        while (messages.TryDequeue(out line)) {
            if (line.Contains(" | frames ")) {
                if (!stopping) status.Text = line;
            } else {
                if (activity.TextLength > 20000) activity.Text = activity.Text.Substring(10000);
                activity.AppendText(line + Environment.NewLine);
            }
        }
        if (!engine.HasExited || Interlocked.CompareExchange(ref closedPipes, 0, 0) != 2 || !messages.IsEmpty) return;
        status.Text = engine.ExitCode == 0 ? "Finished" : "Failed - see activity below";
        engine.Dispose();
        engine = null;
        UpdateControls();
        if (closing) Close();
    }
}
