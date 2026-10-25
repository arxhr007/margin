using System.Drawing;

namespace Margin.Touchpad;

/// One finger on the touchpad, in normalized 0..1 coordinates (origin bottom-left: y=1 is the top edge).
internal readonly record struct TouchPoint(int Id, PointF Start, PointF Current);

/// Fingers currently on the touchpad plus the state of its physical button.
/// Everything runs on the UI thread (raw input and the mouse hook share it), so no locking.
internal sealed class TouchTracker
{
    private readonly Dictionary<int, TouchPoint> touches = new();

    /// True while the touchpad reports its button pressed (a real click, not tap-to-click).
    public bool ButtonDown { get; private set; }

    /// False for touchpads whose HID report has no button usage; then "finger is down" is the click test.
    public bool DeviceHasButton { get; set; }

    /// Raised after each complete touch frame.
    public event Action? Frame;

    public IReadOnlyList<TouchPoint> Snapshot() => touches.Values.ToList();

    public int Count => touches.Count;

    /// Applies one complete frame: the fingers that are down right now (tip switch on).
    public void Commit(IReadOnlyList<(int Id, float X, float Y)> down, bool button)
    {
        ButtonDown = button;
        var seen = new HashSet<int>();
        foreach (var (id, x, y) in down)
        {
            seen.Add(id);
            var p = new PointF(x, y);
            touches[id] = touches.TryGetValue(id, out var existing)
                ? existing with { Current = p }
                : new TouchPoint(id, p, p);
        }
        foreach (var gone in touches.Keys.Where(k => !seen.Contains(k)).ToList())
            touches.Remove(gone);

        Frame?.Invoke();
    }

    public void Clear()
    {
        touches.Clear();
        ButtonDown = false;
    }
}
