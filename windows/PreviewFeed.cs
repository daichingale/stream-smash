using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Runtime.InteropServices;
using System.Threading;

sealed class PreviewFeed : IDisposable
{
    readonly TcpListener listener = new TcpListener(IPAddress.Loopback, 0);
    readonly object gate = new object();
    readonly int width, height;
    readonly Action notify;
    byte[] incoming, waiting, painting;
    TcpClient client;
    bool ready, stopped;
    volatile bool disconnected;
    public bool Disconnected { get { return disconnected; } }
    public int Port { get; private set; }

    public PreviewFeed(int width, int height, Action notify)
    {
        this.width = width;
        this.height = height;
        this.notify = notify;
        incoming = new byte[width * height * 4];
        waiting = new byte[incoming.Length];
        painting = new byte[incoming.Length];
        listener.Start(1);
        Port = ((IPEndPoint)listener.LocalEndpoint).Port;
        new Thread(Receive) { IsBackground = true }.Start();
    }

    void Receive()
    {
        try {
            using (var connection = listener.AcceptTcpClient()) {
                lock (gate) {
                    if (stopped) return;
                    client = connection;
                }
                listener.Stop();
                using (var stream = connection.GetStream()) {
                    for (;;) {
                        for (int offset = 0; offset < incoming.Length;) {
                            int count = stream.Read(incoming, offset, incoming.Length - offset);
                            if (count == 0) return;
                            offset += count;
                        }
                        lock (gate) {
                            if (stopped) return;
                            var old = waiting;
                            waiting = incoming;
                            incoming = old;
                            ready = true;
                        }
                        notify();
                    }
                }
            }
        } catch (SocketException) {
        } catch (IOException) {
        } catch (ObjectDisposedException) {
        } finally {
            disconnected = true;
            notify();
        }
    }

    public bool ReadLatest(Bitmap bitmap)
    {
        lock (gate) {
            if (!ready || stopped) return false;
            var old = painting;
            painting = waiting;
            waiting = old;
            ready = false;
        }
        var data = bitmap.LockBits(new Rectangle(0, 0, width, height), ImageLockMode.WriteOnly, PixelFormat.Format32bppRgb);
        try {
            Marshal.Copy(painting, 0, data.Scan0, painting.Length);
        } finally {
            bitmap.UnlockBits(data);
        }
        return true;
    }

    public void Dispose()
    {
        lock (gate) {
            stopped = true;
            listener.Stop();
            if (client != null) client.Close();
        }
    }
}
