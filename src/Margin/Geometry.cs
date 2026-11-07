using System.Drawing;

namespace Margin;

/// Where each section sits on the touchpad. Coordinates are normalized 0..1 with y = 1 at the top.
/// Corners own their squares; edges run between the corners. The settings window draws exactly
/// these rectangles, so what you see there is what the touchpad does.
internal static class Geometry
{
    public static readonly Section[] All =
    {
        Section.TopLeft, Section.TopRight, Section.BottomLeft, Section.BottomRight,
        Section.Left, Section.Right, Section.Top, Section.Bottom,
    };

    public static bool IsCorner(this Section s) => s is Section.TopLeft or Section.TopRight or Section.BottomLeft or Section.BottomRight;

    /// Left and right edges are slid up/down; top and bottom left/right.
    public static bool IsVertical(this Section s) => s is Section.Left or Section.Right;

    public static RectangleF Bounds(Section s, Settings st)
    {
        float c = (float)st.CornerSize, d = (float)st.EdgeDepth;
        return s switch
        {
            Section.TopLeft => new RectangleF(0, 1 - c, c, c),
            Section.TopRight => new RectangleF(1 - c, 1 - c, c, c),
            Section.BottomLeft => new RectangleF(0, 0, c, c),
            Section.BottomRight => new RectangleF(1 - c, 0, c, c),
            Section.Left => new RectangleF(0, c, d, 1 - 2 * c),
            Section.Right => new RectangleF(1 - d, c, d, 1 - 2 * c),
            Section.Top => new RectangleF(c, 1 - d, 1 - 2 * c, d),
            _ => new RectangleF(c, 0, 1 - 2 * c, d), // Bottom
        };
    }

    private static bool Inside(RectangleF r, PointF p) =>
        p.X >= r.Left && p.X <= r.Right && p.Y >= r.Top && p.Y <= r.Bottom;

    /// The section under a point, assigned or not.
    public static Section? Hit(PointF p, Settings st)
    {
        foreach (var s in All)
            if (Inside(Bounds(s, st), p)) return s;
        return null;
    }

    public static bool IsAssigned(Section s, Settings st) =>
        s.IsCorner() ? st[s].Clicks.Any(c => c.Do != ClickAction.None) : st[s].Slide != SlideAction.Off;

    /// Slide-in mode: the assigned edge whose thin outer strip contains a newborn finger.
    public static Section? EntryEdge(PointF p, Settings st)
    {
        float e = (float)st.EntryStrip, c = (float)st.CornerSize;
        bool alongSide = p.Y >= c && p.Y <= 1 - c;
        bool alongTop = p.X >= c && p.X <= 1 - c;
        Section? s = null;
        if (p.X <= e && alongSide) s = Section.Left;
        else if (p.X >= 1 - e && alongSide) s = Section.Right;
        else if (p.Y <= e && alongTop) s = Section.Bottom;
        else if (p.Y >= 1 - e && alongTop) s = Section.Top;
        return s is { } edge && IsAssigned(edge, st) ? edge : null;
    }

    /// How far from its edge a finger may drift before an active slide ends.
    public static bool InCorridor(Section s, PointF p, Settings st)
    {
        float limit = (float)Math.Max(st.EdgeDepth, 0.08) * 1.75f;
        return s switch
        {
            Section.Left => p.X <= limit,
            Section.Right => p.X >= 1 - limit,
            Section.Bottom => p.Y <= limit,
            _ => p.Y >= 1 - limit, // Top
        };
    }

    /// Position along the slide axis (up and right are positive).
    public static float Along(Section s, PointF p) => s.IsVertical() ? p.Y : p.X;

    /// Distance travelled away from the edge, toward the middle.
    public static float Inward(Section s, PointF from, PointF to) => s switch
    {
        Section.Left => to.X - from.X,
        Section.Right => from.X - to.X,
        Section.Bottom => to.Y - from.Y,
        _ => from.Y - to.Y,
    };

    public static string Name(this Section s) => s switch
    {
        Section.Left => "Left edge",
        Section.Right => "Right edge",
        Section.Top => "Top edge",
        Section.Bottom => "Bottom edge",
        Section.TopLeft => "Top-left corner",
        Section.TopRight => "Top-right corner",
        Section.BottomLeft => "Bottom-left corner",
        _ => "Bottom-right corner",
    };
}
