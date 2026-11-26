using System;
using System.Drawing;
using System.IO;
using System.Windows.Forms;

sealed class PreviewPane : Panel
{
    readonly PictureBox picture = new PictureBox {
        BackColor = Color.FromArgb(16, 19, 25), Dock = DockStyle.Fill, SizeMode = PictureBoxSizeMode.Zoom
    };
    readonly Panel dock = new Panel { Dock = DockStyle.Fill };
    readonly Button detach = new Button { Text = "別ウィンドウ", AutoSize = true };
    readonly CheckBox topmost = new CheckBox { Text = "最前面", AutoSize = true, Enabled = false, Padding = new Padding(4) };
    readonly Label state = new Label { Text = "入力を選び、プレビュー開始を押してください。", Dock = DockStyle.Bottom, Height = 30 };
    Form floating;
    string imagePath;
    DateTime latest;

    public PreviewPane()
    {
        Dock = DockStyle.Fill;
        var actions = new FlowLayoutPanel { Dock = DockStyle.Top, AutoSize = true };
        actions.Controls.AddRange(new Control[] { detach, topmost });
        Controls.Add(dock);
        Controls.Add(state);
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

    public void Begin()
    {
        End();
        imagePath = Path.Combine(Path.GetTempPath(), "StreamSmash-" + Guid.NewGuid().ToString("N") + ".jpg");
        state.Text = "映像を待っています…";
    }

    public string ImagePath { get { return imagePath; } }

    public void ReadFrame()
    {
        if (imagePath == null || !picture.Visible) return;
        try {
            DateTime modified = File.GetLastWriteTimeUtc(imagePath);
            if (modified == latest) return;
            using (var file = new FileStream(imagePath, FileMode.Open, FileAccess.Read, FileShare.ReadWrite | FileShare.Delete))
            using (var decoded = Image.FromStream(file)) {
                ReplaceImage(new Bitmap(decoded));
            }
            latest = modified;
            state.Text = "出力映像のプレビュー";
        } catch (IOException) {
            // The first frame may not exist yet, or FFmpeg may be replacing it.
        } catch (ArgumentException) {
            // An incomplete frame is skipped until the next refresh.
        } catch (UnauthorizedAccessException) {
            state.Text = "プレビュー画像を読み込めません。";
        }
    }

    public void End()
    {
        if (imagePath != null) {
            DeleteFrame(imagePath);
            DeleteFrame(imagePath + ".tmp");
        }
        imagePath = null;
        latest = DateTime.MinValue;
        ReplaceImage(null);
        state.Text = "入力を選び、プレビュー開始を押してください。";
    }

    static void DeleteFrame(string path)
    {
        try { File.Delete(path); }
        catch (IOException) {}
        catch (UnauthorizedAccessException) {}
    }

    void ReplaceImage(Image image)
    {
        var previous = picture.Image;
        picture.Image = image;
        if (previous != null) previous.Dispose();
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) {
            End();
            if (floating != null) floating.Close();
        }
        base.Dispose(disposing);
    }
}
