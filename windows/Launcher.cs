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
    readonly ComboBox source = new ComboBox();
    readonly ComboBox audio = new ComboBox();
    readonly TextBox mediaFile = new TextBox();
    readonly ComboBox resolution = new ComboBox();
    readonly ComboBox buffer = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList };
    readonly NumericUpDown fps = new NumericUpDown { Minimum = 1, Maximum = 120, Value = 60 };
    readonly TextBox folder = new TextBox { Text = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.MyVideos), "Stream Smash") };
    readonly TextBox rtmp = new TextBox { UseSystemPasswordChar = true };
    readonly Button browse = new Button { Text = "動画を選ぶ", AutoSize = true };
    readonly Button devices = new Button { Text = "機器を検索", AutoSize = true };
    readonly Button chooseFolder = new Button { Text = "保存先を選ぶ", AutoSize = true };
    readonly CheckBox audioPeaks = new CheckBox { Text = "音の盛り上がりを自動保存", AutoSize = true, Margin = new Padding(12, 7, 3, 3) };
    readonly Button start = new Button { Text = "録画開始", AutoSize = true, BackColor = Color.FromArgb(220, 232, 255) };
    readonly Button stop = new Button { Text = "停止", AutoSize = true };
    readonly Button save = new Button { Text = "見せ場を保存（F8）", AutoSize = true };
    readonly Button openClip = new Button { Text = "最新クリップを開く", AutoSize = true, Enabled = false };
    readonly Label status = new Label { Text = "準備完了", AutoSize = true, Margin = new Padding(3, 5, 3, 5) };
    readonly Label clipStatus = new Label { AutoSize = true, Margin = new Padding(3, 0, 3, 5) };
    readonly TextBox activity = new TextBox { Multiline = true, ReadOnly = true, ScrollBars = ScrollBars.Both, WordWrap = false, Dock = DockStyle.Fill };
    readonly System.Windows.Forms.Timer timer = new System.Windows.Forms.Timer { Interval = 100 };
    readonly ConcurrentQueue<string> messages = new ConcurrentQueue<string>();
    static readonly Regex Progress = new Regex(@"^(\S+) \| frames (\d+) \| encoder dropped (\d+) \| duplicated (\d+) \| RTMP retries (\d+)$");
    Process engine;
    int closedPipes;
    bool recording, stopping, closing, highlightPending, hasScanned;
    string latestClip;

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
        ClientSize = new Size(828, 585);
        MinimumSize = new Size(700, 515);
        Font = new Font("Segoe UI", 10);
        KeyPreview = true;
        var layout = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(16), ColumnCount = 2, RowCount = 13 };
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 161));
        layout.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        for (int row = 0; row < 12; row++) layout.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        layout.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        Controls.Add(layout);
        mode.Items.AddRange(new object[] { "テスト映像・音声", "キャプチャー機器", "動画ファイル" });
        mode.SelectedIndex = 0;
        source.MaxDropDownItems = audio.MaxDropDownItems = 14;
        resolution.Items.AddRange(new object[] { "1280x720", "1920x1080", "2560x1440", "3840x2160" });
        resolution.Text = "1920x1080";
        buffer.Items.AddRange(new object[] { "オフ", "10 秒", "30 秒", "60 秒" });
        buffer.SelectedIndex = 2;
        var inputs = new Panel { Height = source.PreferredHeight };
        source.Dock = mediaFile.Dock = DockStyle.Top;
        inputs.Controls.Add(source);
        inputs.Controls.Add(mediaFile);
        AddField(layout, 0, "入力", mode);
        AddField(layout, 1, "映像機器／動画", inputs);
        AddField(layout, 2, "音声機器", audio);
        AddField(layout, 3, "解像度", resolution);
        AddField(layout, 4, "フレームレート", fps);
        AddField(layout, 5, "録画先", folder);
        AddField(layout, 6, "配信 URL（任意）", rtmp);
        AddField(layout, 7, "見せ場の長さ", buffer);
        AddWide(layout, 8, Actions(devices, browse, chooseFolder, audioPeaks));
        AddWide(layout, 9, Actions(start, stop, save, openClip));
        AddWide(layout, 10, status);
        AddWide(layout, 11, clipStatus);
        AddWide(layout, 12, activity);
        mode.SelectedIndexChanged += (sender, args) => {
            UpdateControls();
            if (mode.SelectedIndex == 1 && !hasScanned && engine == null) ScanDevices();
        };
        rtmp.TextChanged += (sender, args) => start.Text = string.IsNullOrWhiteSpace(rtmp.Text) ? "録画開始" : "録画と配信を開始";
        buffer.SelectedIndexChanged += (sender, args) => UpdateControls();
        devices.Click += (sender, args) => ScanDevices();
        browse.Click += (sender, args) => {
            using (var dialog = new OpenFileDialog())
                if (dialog.ShowDialog(this) == DialogResult.OK) mediaFile.Text = dialog.FileName;
        };
        chooseFolder.Click += (sender, args) => {
            using (var dialog = new FolderBrowserDialog { SelectedPath = folder.Text })
                if (dialog.ShowDialog(this) == DialogResult.OK) folder.Text = dialog.SelectedPath;
        };
        start.Click += (sender, args) => StartRecording();
        stop.Click += (sender, args) => StopRecording();
        save.Click += (sender, args) => SaveHighlight();
        openClip.Click += (sender, args) => {
            if (!File.Exists(latestClip)) { clipStatus.Text = "クリップが見つかりません。"; return; }
            try { Process.Start(new ProcessStartInfo(latestClip) { UseShellExecute = true }); }
            catch (System.ComponentModel.Win32Exception error) { clipStatus.Text = error.Message; }
        };
        KeyDown += (sender, args) => {
            if (args.KeyCode != Keys.F8 || !save.Enabled) return;
            args.SuppressKeyPress = true;
            SaveHighlight();
        };
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

    static FlowLayoutPanel Actions(params Control[] controls)
    {
        var panel = new FlowLayoutPanel { AutoSize = true, Dock = DockStyle.Fill, WrapContents = true };
        panel.Controls.AddRange(controls);
        return panel;
    }

    void ScanDevices()
    {
        hasScanned = true;
        source.Items.Clear();
        audio.Items.Clear();
        Launch(new[] { "devices", "--names" }, false, "");
    }

    void UpdateControls()
    {
        bool idle = engine == null;
        foreach (var control in new Control[] { mode, resolution, fps, folder, rtmp, buffer, chooseFolder, start }) control.Enabled = idle;
        source.Visible = mode.SelectedIndex != 2;
        mediaFile.Visible = mode.SelectedIndex == 2;
        source.Enabled = idle && mode.SelectedIndex == 1;
        mediaFile.Enabled = idle && mode.SelectedIndex == 2;
        audio.Enabled = idle && mode.SelectedIndex == 1;
        devices.Enabled = idle && mode.SelectedIndex == 1;
        browse.Enabled = idle && mode.SelectedIndex == 2;
        stop.Enabled = !idle && recording && !stopping;
        save.Enabled = stop.Enabled && buffer.SelectedIndex > 0 && !highlightPending;
        audioPeaks.Enabled = idle && buffer.SelectedIndex > 0;
    }

    void StartRecording()
    {
        string video = mode.SelectedIndex == 2 ? mediaFile.Text.Trim() : source.Text.Trim();
        if ((mode.SelectedIndex != 0 && video.Length == 0)
            || string.IsNullOrWhiteSpace(folder.Text) || string.IsNullOrWhiteSpace(resolution.Text)) {
            status.Text = "入力、解像度、録画先を指定してください。";
            return;
        }
        var args = new List<string> { "record", "--size", resolution.Text.Trim(), "--fps", fps.Value.ToString(), "--output-dir", folder.Text.Trim() };
        switch (mode.SelectedIndex) {
            case 0: args.Add("--test-pattern"); break;
            case 1:
                args.AddRange(new[] { "--video-device", video });
                if (!string.IsNullOrWhiteSpace(audio.Text)) args.AddRange(new[] { "--audio-device", audio.Text.Trim() });
                break;
            case 2: args.AddRange(new[] { "--input", video }); break;
        }
        if (buffer.SelectedIndex > 0) {
            string[] lengths = { "", "10", "30", "60" };
            args.AddRange(new[] { "--highlights", lengths[buffer.SelectedIndex] });
            if (audioPeaks.Checked) args.Add("--audio-peaks");
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
        highlightPending = false;
        status.Text = record ? "録画を開始しています…" : "接続機器を検索しています…";
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
            status.Text = "録画を保存しています…";
            UpdateControls();
        } catch (IOException error) { status.Text = error.Message; }
    }

    void SaveHighlight()
    {
        try {
            engine.StandardInput.WriteLine("h");
            engine.StandardInput.Flush();
            highlightPending = true;
            clipStatus.Text = "見せ場を保存しています…";
            UpdateControls();
        } catch (IOException error) { clipStatus.Text = error.Message; }
    }

    void ReadActivity()
    {
        if (engine == null) return;
        string line;
        while (messages.TryDequeue(out line)) {
            if (!recording && (line.StartsWith("video\t") || line.StartsWith("audio\t"))) {
                var choices = line.StartsWith("video\t") ? source : audio;
                choices.Items.Add(line.Substring(6));
                continue;
            }
            bool manualClip = line.StartsWith("Highlight saved: ", StringComparison.Ordinal);
            if (manualClip || line.StartsWith("Audio highlight saved: ", StringComparison.Ordinal)) {
                string name = Path.GetFileName(line.Substring(line.IndexOf(": ", StringComparison.Ordinal) + 2));
                latestClip = Path.Combine(Path.GetFullPath(folder.Text), name);
                clipStatus.Text = (manualClip ? "見せ場を保存しました　" : "音の盛り上がりを自動保存しました　") + name;
                openClip.Enabled = true;
                if (manualClip) highlightPending = false;
                UpdateControls();
            } else if (line.StartsWith("Highlight unavailable: ")) {
                clipStatus.Text = "クリップを保存できませんでした。詳細はログを確認してください。";
                highlightPending = false;
                UpdateControls();
            }
            var progress = Progress.Match(line);
            if (progress.Success) {
                if (!stopping) {
                    var fields = progress.Groups;
                    status.Text = string.Format("{0}  ｜  {1} フレーム  ｜  間引き {2}・重複 {3}  ｜  配信再試行 {4}",
                        fields[1].Value.Split('.')[0], fields[2], fields[3], fields[4], fields[5]);
                }
            } else {
                if (activity.TextLength > 20000) activity.Text = activity.Text.Substring(10000);
                activity.AppendText(line + Environment.NewLine);
            }
        }
        if (!engine.HasExited || Interlocked.CompareExchange(ref closedPipes, 0, 0) != 2 || !messages.IsEmpty) return;
        status.Text = engine.ExitCode == 0
            ? (recording ? "録画を保存しました" : string.Format("映像機器 {0} 台・音声機器 {1} 台", source.Items.Count, audio.Items.Count))
            : "処理に失敗しました。詳細はログを確認してください。";
        engine.Dispose();
        engine = null;
        UpdateControls();
        if (closing) Close();
    }
}
