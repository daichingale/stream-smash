using System;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Imaging;
using System.Threading;
using System.Windows.Forms;

sealed class PreviewPane : Panel
{
    readonly PictureBox picture = new PictureBox {
        BackColor = Color.FromArgb(16, 19, 25), Dock = DockStyle.Fill, SizeMode = PictureBoxSizeMode.Zoom
    };
    readonly Panel dock = new Panel { Dock = DockStyle.Fill };
    readonly Button detach = new Button { Text = "別ウィンドウ", AutoSize = true };
    readonly CheckBox topmost = new CheckBox { Text = "最前面", AutoSize = true, Enabled = false, Padding = new Padding(4) };
    readonly ComboBox resolution = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Width = 100 };
    readonly NumericUpDown fps = new NumericUpDown { Minimum = 1, Maximum = 60, Value = 30, Width = 57 };
    readonly ComboBox quality = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Width = 92 };
    readonly Label state = new Label { Text = "入力を選び、プレビュー開始を押してください。", Dock = DockStyle.Bottom, Height = 30 };
    Form floating;
    Form sharing;
    PictureBox sharePicture;
    PreviewFeed feed;
    Bitmap frame;
    readonly Stopwatch displayClock = new Stopwatch();
    int displayed, scheduled;

    public PreviewPane()
    {
        Dock = DockStyle.Fill;
        Padding = new Padding(10);
        BackColor = StudioColors.Panel;
        var actions = new FlowLayoutPanel { Dock = DockStyle.Top, AutoSize = true };
        actions.Controls.AddRange(new Control[] { detach, topmost });
        resolution.Items.AddRange(new object[] { "426x240", "640x360", "960x540", "1280x720", "1920x1080" });
        resolution.SelectedIndex = 1;
        quality.Items.AddRange(new object[] { "軽量", "精細" });
        quality.SelectedIndex = 0;
        var settings = new FlowLayoutPanel { Dock = DockStyle.Bottom, AutoSize = true };
        settings.Controls.AddRange(new Control[] {
            Caption("解像度", resolution), resolution, Caption("fps", fps), fps,
            Caption("拡縮品質", quality), quality
        });
        Controls.Add(dock);
        Controls.Add(state);
        Controls.Add(settings);
        Controls.Add(actions);
        dock.Controls.Add(picture);
        detach.Click += (sender, args) => {
            if (floating != null) floating.Close();
            else Detach();
        };
        topmost.CheckedChanged += (sender, args) => {
            if (floating != null) floating.TopMost = topmost.Checked;
        };
    }

    static Label Caption(string title, Control field)
    {
        field.AccessibleName = title;
        return new Label { Text = title, AutoSize = true, Margin = new Padding(3, 6, 3, 3) };
    }

    public bool SettingsEnabled
    {
        set { resolution.Enabled = fps.Enabled = quality.Enabled = value; }
    }

    public string[] Arguments()
    {
        return new[] {
            "--preview-port", feed.Port.ToString(), "--preview-size", resolution.Text,
            "--preview-fps", fps.Value.ToString(), "--preview-quality", quality.SelectedIndex == 0 ? "84" : "95"
        };
    }

    void Detach()
    {
        floating = new Form {
            Text = "Stream Smash — 配信プレビュー",
            ClientSize = new Size(640, 360), MinimumSize = new Size(240, 160),
            StartPosition = FormStartPosition.CenterParent, TopMost = topmost.Checked
        };
        floating.Controls.Add(picture);
        floating.FormClosed += (sender, args) => {
            dock.Controls.Add(picture);
            floating = null;
            detach.Text = "別ウィンドウ";
            topmost.Enabled = false;
        };
        detach.Text = "メイン画面に戻す";
        topmost.Enabled = true;
        floating.Show(FindForm());
    }

    public void ShowShareWindow()
    {
        if (sharing != null) {
            if (sharing.WindowState == FormWindowState.Minimized) sharing.WindowState = FormWindowState.Normal;
            sharing.Activate();
            return;
        }
        sharing = new Form {
            Text = "Stream Smash - 画面共有",
            ClientSize = new Size(960, 540), MinimumSize = new Size(320, 200),
            StartPosition = FormStartPosition.CenterScreen, ShowInTaskbar = true
        };
        sharePicture = new PictureBox {
            Dock = DockStyle.Fill, BackColor = Color.Black, SizeMode = PictureBoxSizeMode.Zoom,
            Image = picture.Image
        };
        sharing.Controls.Add(sharePicture);
        sharing.FormClosed += (sender, args) => {
            sharePicture.Image = null;
            sharePicture = null;
            sharing = null;
        };
        // Expose an independent window to the screen-sharing picker.
        sharing.Show();
    }

    public void Begin()
    {
        End();
        var dimensions = resolution.Text.Split('x');
        int width = int.Parse(dimensions[0]), height = int.Parse(dimensions[1]);
        feed = new PreviewFeed(width, height, QueueFrame);
        frame = new Bitmap(width, height, PixelFormat.Format32bppRgb);
        displayed = 0;
        displayClock.Reset();
        state.Text = "映像を待っています…";
    }

    void QueueFrame()
    {
        if (Interlocked.Exchange(ref scheduled, 1) != 0) return;
        try {
            BeginInvoke((MethodInvoker)delegate {
                Interlocked.Exchange(ref scheduled, 0);
                if (!IsDisposed) ReadFrame();
            });
        } catch (InvalidOperationException) {
            Interlocked.Exchange(ref scheduled, 0);
        }
    }

    void ReadFrame()
    {
        if (feed == null || (!picture.Visible && sharePicture == null)) return;
        if (feed.ReadLatest(frame)) {
            if (picture.Image == null) {
                picture.Image = frame;
                state.Text = "出力プレビュー";
                displayClock.Start();
            }
            picture.Invalidate();
            if (sharePicture != null) {
                if (sharePicture.Image == null) sharePicture.Image = frame;
                sharePicture.Invalidate();
            }
            displayed++;
        }
        if (feed.Disconnected) {
            state.Text = "プレビュー接続が終了しました。";
        } else if (displayClock.ElapsedMilliseconds >= 1000) {
            state.Text = string.Format("出力プレビュー  •  表示更新 {0:F1} fps", displayed / displayClock.Elapsed.TotalSeconds);
            displayed = 0;
            displayClock.Restart();
        }
    }

    public void End()
    {
        if (feed != null) feed.Dispose();
        feed = null;
        picture.Image = null;
        if (sharePicture != null) sharePicture.Image = null;
        if (frame != null) frame.Dispose();
        frame = null;
        state.Text = "入力を選び、プレビュー開始を押してください。";
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) {
            End();
            if (floating != null) floating.Close();
            if (sharing != null) sharing.Close();
        }
        base.Dispose(disposing);
    }
}
