using System;
using System.Collections.Generic;
using System.Drawing;
using System.Text;

// Fixed version 3-L, byte mode. The longest supported IPv4 pairing URL is 51 bytes.
sealed class PairingQr
{
    const int Size = 29, DataLength = 55, ParityLength = 15;
    readonly bool[,] dark = new bool[Size, Size];
    readonly bool[,] reserved = new bool[Size, Size];

    public static Bitmap Draw(string value)
    {
        var qr = new PairingQr();
        qr.Patterns();
        qr.Fill(Encode(Encoding.ASCII.GetBytes(value)));
        const int pixel = 7, border = 4;
        var bitmap = new Bitmap((Size + 2 * border) * pixel, (Size + 2 * border) * pixel);
        using (var art = Graphics.FromImage(bitmap)) {
            art.Clear(Color.White);
            for (int y = 0; y < Size; y++)
                for (int x = 0; x < Size; x++)
                    if (qr.dark[x, y]) art.FillRectangle(Brushes.Black, (x + border) * pixel, (y + border) * pixel, pixel, pixel);
        }
        return bitmap;
    }

    void Set(int x, int y, bool value)
    {
        if (x < 0 || y < 0 || x >= Size || y >= Size) return;
        dark[x, y] = value;
        reserved[x, y] = true;
    }

    void Finder(int cx, int cy, int radius)
    {
        for (int dy = -radius; dy <= radius; dy++) {
            for (int dx = -radius; dx <= radius; dx++) {
                int distance = Math.Max(Math.Abs(dx), Math.Abs(dy));
                Set(cx + dx, cy + dy, radius == 4 ? distance != 2 && distance != 4 : distance != 1);
            }
        }
    }

    void Patterns()
    {
        for (int i = 0; i < Size; i++) { Set(6, i, i % 2 == 0); Set(i, 6, i % 2 == 0); }
        Finder(3, 3, 4); Finder(Size - 4, 3, 4); Finder(3, Size - 4, 4);
        Finder(22, 22, 2);
        const int format = 0x77c4; // Level L, mask 0, including BCH and the format XOR mask.
        Func<int, bool> bit = i => ((format >> i) & 1) != 0;
        for (int i = 0; i < 6; i++) Set(8, i, bit(i));
        Set(8, 7, bit(6)); Set(8, 8, bit(7)); Set(7, 8, bit(8));
        for (int i = 9; i < 15; i++) Set(14 - i, 8, bit(i));
        for (int i = 0; i < 8; i++) Set(Size - 1 - i, 8, bit(i));
        for (int i = 8; i < 15; i++) Set(8, Size - 15 + i, bit(i));
        Set(8, Size - 8, true);
    }

    void Fill(byte[] data)
    {
        int index = 0;
        bool up = true;
        for (int right = Size - 1; right >= 1; right -= 2) {
            if (right == 6) right--;
            for (int row = 0; row < Size; row++) {
                int y = up ? Size - 1 - row : row;
                for (int x = right; x >= right - 1; x--) {
                    if (reserved[x, y]) continue;
                    bool value = index < data.Length * 8 && ((data[index / 8] >> (7 - index % 8)) & 1) != 0;
                    dark[x, y] = value ^ ((x + y) % 2 == 0);
                    index++;
                }
            }
            up = !up;
        }
    }

    static byte[] Encode(byte[] input)
    {
        if (input.Length > 53) throw new ArgumentException("Pairing URL is too long");
        var bits = new List<int>();
        Add(bits, 4, 4);
        Add(bits, input.Length, 8);
        foreach (byte b in input) Add(bits, b, 8);
        Add(bits, 0, 4);
        var data = new byte[DataLength + ParityLength];
        for (int i = 0; i < bits.Count; i++) data[i / 8] |= (byte)(bits[i] << (7 - i % 8));
        for (int i = bits.Count / 8; i < DataLength; i++) data[i] = (i - bits.Count / 8) % 2 == 0 ? (byte)236 : (byte)17;
        var divisor = new byte[ParityLength];
        divisor[ParityLength - 1] = 1;
        byte root = 1;
        for (int i = 0; i < ParityLength; i++) {
            for (int j = 0; j < divisor.Length; j++) {
                divisor[j] = Multiply(divisor[j], root);
                if (j + 1 < divisor.Length) divisor[j] ^= divisor[j + 1];
            }
            root = Multiply(root, 2);
        }
        var remainder = new byte[ParityLength];
        for (int i = 0; i < DataLength; i++) {
            byte factor = (byte)(data[i] ^ remainder[0]);
            Array.Copy(remainder, 1, remainder, 0, ParityLength - 1);
            remainder[ParityLength - 1] = 0;
            for (int j = 0; j < remainder.Length; j++) remainder[j] ^= Multiply(divisor[j], factor);
        }
        Array.Copy(remainder, 0, data, DataLength, ParityLength);
        return data;
    }

    static void Add(List<int> bits, int value, int count)
    {
        for (int i = count - 1; i >= 0; i--) bits.Add((value >> i) & 1);
    }

    static byte Multiply(byte a, byte b)
    {
        int result = 0;
        for (int i = 7; i >= 0; i--) result = (result << 1) ^ ((result >> 7) * 0x11d) ^ (((b >> i) & 1) * a);
        return (byte)result;
    }
}
