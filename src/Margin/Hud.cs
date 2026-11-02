using System.Drawing.Drawing2D;

namespace Margin;

/// Small floating label for scrubbing and brightness. Volume uses the native Windows flyout.
/// Never takes focus and lets clicks pass through.
internal sealed class Hud : Form
{
    private readonly System.Windows.Forms.Timer hideTimer = new();
    private string text = "";
    private int level = -1; // 0..100 draws a bar, -1 = text only

    public Hud()
    {
        FormBorderStyle = FormBorderStyle.None;
        ShowInTaskbar = false;
        StartPosition = FormStartPosition.Manual;
        TopMost = true;
        BackColor = Color.FromArgb(30, 30, 32);
        Opacity = 0.94;
        DoubleBuffered = true;
        hideTimer.Tick += (_, _) => { hideTimer.Stop(); Native.ShowWindow(Handle, Native.SW_HIDE); };
        _ = Handle; // create the window now, on the UI thread
    }

    protected override bool ShowWithoutActivation => true;

    protected override CreateParams CreateParams
    {
        get
        {
            var cp = base.CreateParams;
            cp.ExStyle |= 0x08000000 /* NOACTIVATE */ | 0x80 /* TOOLWINDOW */ | 0x20 /* TRANSPARENT */ | 0x8 /* TOPMOST */;
            return cp;
        }
    }

    public void ShowText(string label) => Present(label, -1);

    public void ShowLevel(string label, int percent) => Present(label, Math.Clamp(percent, 0, 100));

    private void Present(string label, int percent)
    {
        hideTimer.Stop();
        text = label;
        level = percent;

        float scale = DeviceDpi / 96f;
        var size = new Size((int)(220 * scale), (int)(percent >= 0 ? 70 * scale : 56 * scale));
        var area = Screen.FromPoint(Cursor.Position).WorkingArea;
        Bounds = new Rectangle(area.Left + (area.Width - size.Width) / 2, area.Bottom - size.Height - (int)(90 * scale), size.Width, size.Height);
        using (var path = Rounded(new Rectangle(Point.Empty, size), (int)(16 * scale)))
            Region = new Region(path);

        Invalidate();
        Native.SetWindowPos(Handle, Native.HWND_TOPMOST, 0, 0, 0, 0, Native.SWP_NOMOVE | Native.SWP_NOSIZE | Native.SWP_NOACTIVATE | Native.SWP_SHOWWINDOW);
        Update();
    }

    public void HideAfter(int milliseconds)
    {
        hideTimer.Stop();
        if (milliseconds <= 0) { Native.ShowWindow(Handle, Native.SW_HIDE); return; }
        hideTimer.Interval = milliseconds;
        hideTimer.Start();
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.ClearTypeGridFit;
        float scale = DeviceDpi / 96f;

        using var font = new Font("Segoe UI Semibold", 15f, FontStyle.Regular, GraphicsUnit.Point);
        var textArea = level >= 0
            ? new RectangleF(0, 6 * scale, Width, 34 * scale)
            : new RectangleF(0, 0, Width, Height);
        using var fmt = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center };
        g.DrawString(text, font, Brushes.White, textArea, fmt);

        if (level >= 0)
        {
            var track = new RectangleF(20 * scale, 46 * scale, Width - 40 * scale, 8 * scale);
            using var trackBrush = new SolidBrush(Color.FromArgb(70, 255, 255, 255));
            using var trackPath = Rounded(Rectangle.Round(track), (int)(4 * scale));
            g.FillPath(trackBrush, trackPath);
            if (level > 0)
            {
                var fill = track with { Width = Math.Max(track.Height, track.Width * level / 100f) };
                using var fillPath = Rounded(Rectangle.Round(fill), (int)(4 * scale));
                g.FillPath(Brushes.White, fillPath);
            }
        }
    }

    private static GraphicsPath Rounded(Rectangle r, int radius)
    {
        int d = Math.Max(1, radius * 2);
        var p = new GraphicsPath();
        p.AddArc(r.Left, r.Top, d, d, 180, 90);
        p.AddArc(r.Right - d, r.Top, d, d, 270, 90);
        p.AddArc(r.Right - d, r.Bottom - d, d, d, 0, 90);
        p.AddArc(r.Left, r.Bottom - d, d, d, 90, 90);
        p.CloseFigure();
        return p;
    }

    protected override void Dispose(bool disposing)
    {
        if (disposing) hideTimer.Dispose();
        base.Dispose(disposing);
    }
}
