using System;
using System.Collections.Generic;
using System.Drawing;
using System.Net;
using System.Net.NetworkInformation;
using System.Net.Sockets;
using System.Windows.Forms;

sealed class MobilePairing : Form
{
    readonly Func<IPAddress, MobileRemote> start;
    readonly Action stop;
    readonly ComboBox interfaces = new ComboBox { Dock = DockStyle.Fill, DropDownStyle = ComboBoxStyle.DropDownList };
    readonly PictureBox image = new PictureBox { Dock = DockStyle.Fill, SizeMode = PictureBoxSizeMode.Zoom, BackColor = Color.White };
    readonly TextBox link = new TextBox { Dock = DockStyle.Fill, ReadOnly = true };
    readonly Button enable = new Button { Text = "リモコンを有効にする", AutoSize = true };
    readonly Button disable = new Button { Text = "無効にする", AutoSize = true, Enabled = false };
    readonly Button copy = new Button { Text = "URLをコピー", AutoSize = true, Enabled = false };

    public MobilePairing(Func<IPAddress, MobileRemote> start, Action stop)
    {
        this.start = start;
        this.stop = stop;
        Text = "スマホリモコン";
        ClientSize = new Size(450, 450);
        MinimumSize = new Size(390, 420);
        StartPosition = FormStartPosition.CenterParent;
        var table = new TableLayoutPanel { Dock = DockStyle.Fill, Padding = new Padding(14), RowCount = 5, ColumnCount = 1 };
        table.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        table.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        table.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        table.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        table.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        var hint = new Label { Text = "PCと同じ信頼できるWi-Fiに接続し、QRを読み取ってください。\nこの画面を閉じるとスマホ操作が無効になります。", AutoSize = true };
        table.Controls.Add(hint, 0, 0);
        table.Controls.Add(interfaces, 0, 1);
        table.Controls.Add(image, 0, 2);
        table.Controls.Add(link, 0, 3);
        var buttons = new FlowLayoutPanel { AutoSize = true, Dock = DockStyle.Fill };
        buttons.Controls.AddRange(new Control[] { enable, disable, copy });
        table.Controls.Add(buttons, 0, 4);
        Controls.Add(table);
        var candidates = new HashSet<IPAddress>();
        foreach (var network in NetworkInterface.GetAllNetworkInterfaces()) {
            if (network.OperationalStatus != OperationalStatus.Up) continue;
            foreach (var ip in network.GetIPProperties().UnicastAddresses) {
                if (ip.Address.AddressFamily != AddressFamily.InterNetwork) continue;
                byte[] b = ip.Address.GetAddressBytes();
                if ((b[0] == 10 || (b[0] == 172 && b[1] >= 16 && b[1] <= 31) || (b[0] == 192 && b[1] == 168))
                    && candidates.Add(ip.Address))
                    interfaces.Items.Add(ip.Address);
            }
        }
        if (interfaces.Items.Count > 0) interfaces.SelectedIndex = 0;
        else hint.Text += "\n利用できるローカルIPv4アドレスが見つかりません。";
        enable.Enabled = interfaces.Items.Count > 0;
        enable.Click += (sender, args) => EnableRemote();
        disable.Click += (sender, args) => DisableRemote();
        copy.Click += (sender, args) => {
            try { Clipboard.SetText(link.Text); }
            catch (System.Runtime.InteropServices.ExternalException) { link.Focus(); link.SelectAll(); }
        };
        FormClosed += (sender, args) => DisableRemote();
    }

    void EnableRemote()
    {
        try {
            var server = start((IPAddress)interfaces.SelectedItem);
            link.Text = server.PairingUrl;
            image.Image = PairingQr.Draw(server.PairingUrl);
            interfaces.Enabled = enable.Enabled = false;
            disable.Enabled = copy.Enabled = true;
        } catch (SocketException error) {
            DisableRemote();
            link.Text = "リモコンを開始できません: " + error.Message;
        }
    }

    void DisableRemote()
    {
        stop();
        var old = image.Image;
        image.Image = null;
        if (old != null) old.Dispose();
        link.Clear();
        interfaces.Enabled = true;
        enable.Enabled = interfaces.Items.Count > 0;
        disable.Enabled = copy.Enabled = false;
    }
}
