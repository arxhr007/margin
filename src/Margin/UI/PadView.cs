using System.Drawing.Drawing2D;
using Margin.Touchpad;

namespace Margin.UI;

/// A drawing of the touchpad with its four edges. Click an edge to select it.
/// Fingers currently on the real touchpad are shown as dots, so you can see where the zones are.
internal sealed class PadView : Control
{
    private static readonly Color Accent = Color.FromArgb(0, 103, 192);

    private readonly Settings settings;
    private readonly TouchTracker tracker;
    private readonly System.Windows.Forms.Timer repaint = new() { Interval = 33 };
    private bool dirty;
    private Section? hover;

    public Section Selected { get; set; } = Section.Right;
    public event Action<Section>? SectionClicked;

    public PadView(Settings settings, TouchTracker tracker)
    {
        this.settings = settings;
        this.tracker = tracker;
        SetStyle(ControlStyles.AllPaintingInWmPaint | ControlStyles.OptimizedDoubleBuffer | ControlStyles.UserPaint | ControlStyles.ResizeRedraw, true);
        tracker.Frame += OnFrame;
        repaint.Tick += (_, _) => { if (dirty) { dirty = false; Invalidate(); } };
        repaint.Start();
        MinimumSize = new Size(360, 240);
        AccessibleName = "Touchpad edges";
    }

    private void OnFrame() => dirty = true;

    /// The touchpad outline: a 1.6:1 rounded rectangle centered in the control.
    private RectangleF PadRect()
    {
        float margin = 12 * DeviceDpi / 96f;
        float w = Width - 2 * margin, h = Height - 2 * margin;
        if (w / h > 1.6f) w = h * 1.6f; else h = w / 1.6f;
        return new RectangleF((Width - w) / 2, (Height - h) / 2, w, h);
    }

    private RectangleF ToScreen(RectangleF n, RectangleF pad) =>
        new(pad.X + n.X * pad.Width, pad.Y + (1 - n.Bottom) * pad.Height, n.Width * pad.Width, n.Height * pad.Height);

    private PointF ToScreen(PointF n, RectangleF pad) => new(pad.X + n.X * pad.Width, pad.Y + (1 - n.Y) * pad.Height);

    private Section? HitTest(Point p)
    {
        var pad = PadRect();
        if (!pad.Contains(p)) return null;
        var n = new PointF((p.X - pad.X) / pad.Width, 1 - (p.Y - pad.Y) / pad.Height);
        return Geometry.Hit(n, settings);
    }

    protected override void OnMouseMove(MouseEventArgs e)
    {
        var h = HitTest(e.Location);
        Cursor = h != null ? Cursors.Hand : Cursors.Default;
        if (h != hover) { hover = h; Invalidate(); }
    }

    protected override void OnMouseLeave(EventArgs e)
    {
        hover = null;
        Invalidate();
    }

    protected override void OnMouseClick(MouseEventArgs e)
    {
        if (e.Button == MouseButtons.Left && HitTest(e.Location) is { } s) SectionClicked?.Invoke(s);
    }

    protected override void OnPaint(PaintEventArgs e)
    {
        var g = e.Graphics;
        g.SmoothingMode = SmoothingMode.AntiAlias;
        g.TextRenderingHint = System.Drawing.Text.TextRenderingHint.ClearTypeGridFit;
        float scale = DeviceDpi / 96f;
        var pad = PadRect();

        using (var path = Rounded(pad, 14 * scale))
        using (var fill = new SolidBrush(Color.FromArgb(242, 243, 245)))
        using (var border = new Pen(Color.FromArgb(160, 160, 165), 1.5f * scale))
        {
            g.FillPath(fill, path);
            g.DrawPath(border, path);
        }

        using var font = new Font(Font.FontFamily, 8.5f);
        using var bold = new Font(Font.FontFamily, 9f, FontStyle.Bold);
        using var center = new StringFormat { Alignment = StringAlignment.Center, LineAlignment = StringAlignment.Center, Trimming = StringTrimming.EllipsisCharacter };

        foreach (var s in Geometry.All)
        {
            var r = ToScreen(Geometry.Bounds(s, settings), pad);
            r.Inflate(-2 * scale, -2 * scale);
            bool assigned = Geometry.IsAssigned(s, settings);
            bool selected = s == Selected;
            int alpha = selected ? 120 : hover == s ? 80 : assigned ? 55 : 0;

            using (var path = Rounded(r, 6 * scale))
            {
                if (alpha > 0)
                {
                    using var b = new SolidBrush(Color.FromArgb(alpha, Accent));
                    g.FillPath(b, path);
                }
                using var pen = new Pen(selected ? Accent : Color.FromArgb(assigned ? 140 : 90, 120, 120, 128), (selected ? 2.5f : 1f) * scale);
                if (!assigned && !selected) pen.DashStyle = DashStyle.Dash;
                g.DrawPath(pen, path);
            }

            var text = Labels.Short(settings[s].Slide);
            if (text.Length > 0)
            {
                using var brush = new SolidBrush(selected ? Color.FromArgb(0, 50, 110) : Color.FromArgb(40, 40, 45));
                var state = g.Save();
                var rc = r;
                if (s.IsVertical())
                {
                    // write sideways along tall, narrow edges
                    g.TranslateTransform(r.X + r.Width / 2, r.Y + r.Height / 2);
                    g.RotateTransform(s == Section.Left ? -90 : 90);
                    rc = new RectangleF(-r.Height / 2, -r.Width / 2, r.Height, r.Width);
                }
                g.DrawString(text, selected ? bold : font, brush, rc, center);
                g.Restore(state);
            }
        }

        using var hint = new SolidBrush(Color.FromArgb(130, 130, 136));
        g.DrawString("Normal touchpad", font, hint, ToScreen(new RectangleF(0.3f, 0.4f, 0.4f, 0.2f), pad), center);

        foreach (var t in tracker.Snapshot())
        {
            var p = ToScreen(t.Current, pad);
            float rad = 9 * scale;
            using var dot = new SolidBrush(Color.FromArgb(200, 255, 140, 0));
            g.FillEllipse(dot, p.X - rad, p.Y - rad, rad * 2, rad * 2);
        }
    }

    private static GraphicsPath Rounded(RectangleF r, float radius)
    {
        float d = Math.Max(1, Math.Min(radius * 2, Math.Min(r.Width, r.Height)));
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
        if (disposing)
        {
            tracker.Frame -= OnFrame;
            repaint.Dispose();
        }
        base.Dispose(disposing);
    }
}
