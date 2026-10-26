using System.Drawing;

namespace Margin;

/// Where each edge sits on the touchpad. Coordinates are normalized 0..1 with y = 1 at the top.
internal static class Geometry
{
    public static readonly Section[] All =
    {
        Section.Left, Section.Right, Section.Top, Section.Bottom,
    };

    /// Left and right edges are slid up/down; top and bottom left/right.
    public static bool IsVertical(this Section s) => s is Section.Left or Section.Right;

    public static RectangleF Bounds(Section s, Settings st)
    {
        float d = (float)st.EdgeDepth;
        return s switch
        {
            Section.Left => new RectangleF(0, 0, d, 1),
            Section.Right => new RectangleF(1 - d, 0, d, 1),
            Section.Top => new RectangleF(0, 1 - d, 1, d),
            _ => new RectangleF(0, 0, 1, d), // Bottom
        };
    }

    private static bool Inside(RectangleF r, PointF p) =>
        p.X >= r.Left && p.X <= r.Right && p.Y >= r.Top && p.Y <= r.Bottom;

    /// The edge under a point, assigned or not.
    public static Section? Hit(PointF p, Settings st)
    {
        foreach (var s in All)
            if (Inside(Bounds(s, st), p)) return s;
        return null;
    }

    public static bool IsAssigned(Section s, Settings st) => st[s].Slide != SlideAction.Off;

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
        _ => "Bottom edge",
    };
}
