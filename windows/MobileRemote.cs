using System;
using System.Collections.Generic;
using System.IO;
using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Threading;

// The phone page and its commands exist only while the desktop enables this listener.
sealed class MobileRemote : IDisposable
{
    readonly TcpListener listener;
    readonly byte[] page;
    readonly string key;
    readonly Action<string> enqueue;
    readonly Func<string> snapshot;
    readonly Func<byte[]> preview;
    readonly object gate = new object();
    readonly HashSet<TcpClient> clients = new HashSet<TcpClient>();
    volatile bool disposed;
    long lastAction;
    readonly System.Diagnostics.Stopwatch clock = System.Diagnostics.Stopwatch.StartNew();

    public string Address { get; private set; }
    public string PairingUrl { get { return Address + "#" + key; } }

    public MobileRemote(IPAddress ip, byte[] page, Action<string> enqueue, Func<string> snapshot, Func<byte[]> preview)
    {
        if (ip.AddressFamily != AddressFamily.InterNetwork) throw new ArgumentException("IPv4 only");
        byte[] secret = new byte[16];
        using (var random = RandomNumberGenerator.Create()) random.GetBytes(secret);
        key = Convert.ToBase64String(secret).TrimEnd('=').Replace('+', '-').Replace('/', '_');
        this.page = page;
        this.enqueue = enqueue;
        this.snapshot = snapshot;
        this.preview = preview;
        listener = new TcpListener(ip, 0);
        listener.Start(8);
        Address = "http://" + listener.LocalEndpoint;
        new Thread(Accept) { IsBackground = true }.Start();
    }

    void Accept()
    {
        while (!disposed) {
            TcpClient client;
            try { client = listener.AcceptTcpClient(); }
            catch (SocketException) { return; }
            catch (ObjectDisposedException) { return; }
            lock (gate) {
                if (disposed || clients.Count >= 8) { client.Close(); continue; }
                clients.Add(client);
            }
            ThreadPool.QueueUserWorkItem(_ => Serve(client));
        }
    }

    void Serve(TcpClient client)
    {
        try {
            client.ReceiveTimeout = client.SendTimeout = 2000;
            var stream = client.GetStream();
            var request = ReadHeaders(stream);
            if (request == null) return;
            var lines = request.Split(new[] { "\r\n" }, StringSplitOptions.None);
            var target = lines[0].Split(' ');
            if (target.Length != 3 || (target[2] != "HTTP/1.1" && target[2] != "HTTP/1.0")) return;
            var headers = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
            for (int i = 1; i < lines.Length && lines[i].Length != 0; i++) {
                int colon = lines[i].IndexOf(':');
                if (colon < 1) return;
                string name = lines[i].Substring(0, colon);
                if (headers.ContainsKey(name)) return;
                headers.Add(name, lines[i].Substring(colon + 1).Trim());
            }
            string host, origin, length;
            if (!headers.TryGetValue("Host", out host) || "http://" + host != Address ||
                (headers.TryGetValue("Origin", out origin) && origin != Address) ||
                headers.ContainsKey("Transfer-Encoding") ||
                (headers.TryGetValue("Content-Length", out length) && length != "0")) {
                Reply(stream, "400 Bad Request");
                return;
            }
            if (target[0] == "GET" && target[1] == "/") {
                Reply(stream, "200 OK", "text/html; charset=utf-8", page);
                return;
            }
            string provided;
            if (!headers.TryGetValue("X-Stream-Smash-Key", out provided) || !EqualKey(provided)) {
                Reply(stream, "403 Forbidden");
                return;
            }
            if (target[0] == "GET" && target[1] == "/status") {
                Reply(stream, "200 OK", "application/json", Encoding.UTF8.GetBytes(snapshot()));
            } else if (target[0] == "GET" && target[1] == "/preview") {
                var image = preview();
                Reply(stream, image == null ? "204 No Content" : "200 OK", "image/jpeg", image);
            } else if (target[0] == "POST" && IsAction(target[1])) {
                bool accepted;
                lock (gate) {
                    if (disposed) return;
                    long now = clock.ElapsedMilliseconds;
                    accepted = lastAction == 0 || now - lastAction >= 300;
                    if (accepted) {
                        lastAction = Math.Max(1, now);
                        enqueue(target[1]);
                    }
                }
                Reply(stream, accepted ? "202 Accepted" : "429 Too Many Requests");
            } else {
                Reply(stream, "404 Not Found");
            }
        } catch (IOException) {
        } catch (SocketException) {
        } catch (ObjectDisposedException) {
        } finally {
            client.Close();
            lock (gate) clients.Remove(client);
        }
    }

    static bool IsAction(string path)
    {
        return path == "/preview-start" || path == "/record" || path == "/stream" || path == "/stop" || path == "/clip";
    }

    bool EqualKey(string value)
    {
        if (value.Length != key.Length) return false;
        int difference = 0;
        for (int i = 0; i < key.Length; i++) difference |= key[i] ^ value[i];
        return difference == 0;
    }

    static string ReadHeaders(NetworkStream stream)
    {
        var bytes = new byte[4096];
        int used = 0;
        var clock = System.Diagnostics.Stopwatch.StartNew();
        while (used < bytes.Length && clock.ElapsedMilliseconds < 2500) {
            int count = stream.Read(bytes, used, bytes.Length - used);
            if (count == 0) return null;
            int current = used + count;
            for (int i = Math.Max(0, used - 3); i + 3 < current; i++)
                if (bytes[i] == 13 && bytes[i + 1] == 10 && bytes[i + 2] == 13 && bytes[i + 3] == 10)
                    return Encoding.ASCII.GetString(bytes, 0, i + 4);
            used = current;
        }
        return null;
    }

    static void Reply(NetworkStream stream, string status, string mime = "text/plain", byte[] body = null)
    {
        var headers = Encoding.ASCII.GetBytes("HTTP/1.1 " + status + "\r\nContent-Type: " + mime +
            "\r\nContent-Length: " + (body == null ? 0 : body.Length) +
            "\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff" +
            "\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: DENY" +
            "\r\nContent-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'; img-src blob:; frame-ancestors 'none'\r\n\r\n");
        stream.Write(headers, 0, headers.Length);
        if (body != null) stream.Write(body, 0, body.Length);
    }

    public void Dispose()
    {
        lock (gate) {
            disposed = true;
            listener.Stop();
            foreach (var client in clients) client.Close();
        }
    }
}
