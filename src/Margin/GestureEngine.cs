using System.Drawing;
using Margin.Input;
using Margin.Touchpad;

namespace Margin;

/// Turns touchpad frames and clicks into edge actions.
///
/// Rest a finger on an assigned edge, press the touchpad until it clicks, then slide while held.
/// The click and the drag are hidden from apps. While a slide runs the pointer is frozen and
/// travel along the edge is turned into steps.
internal sealed class GestureEngine
{
    private sealed class Run
    {
        public required Section Section;
        public required SlideAction Action;
        public required int FingerId;
        public required float Origin;
        public int Steps;
    }

    private readonly TouchTracker touches;
    private readonly Brightness brightness;
    private readonly SynchronizationContext ui;
    private readonly HashSet<int> ignoredFingers = new();

    private Run? run;
    private bool holdingClick;

    public Settings Settings { get; set; } = new();

    /// While true the mouse hook drops pointer movement.
    public bool FreezePointer => run != null;

    public GestureEngine(TouchTracker touches, Brightness brightness)
    {
        this.touches = touches;
        this.brightness = brightness;
        ui = SynchronizationContext.Current ?? throw new InvalidOperationException("create on the UI thread");
        touches.Frame += OnFrame;
    }

    /// Work that may be slow (SendInput, WMI, drawing) runs after the mouse hook has returned.
    private void Post(Action work) => ui.Post(_ =>
    {
        try { work(); }
        catch (Exception e) { Log.Error(e.ToString()); }
    }, null);

    // ---------------- touch frames ----------------

    private void OnFrame()
    {
        var fingers = touches.Snapshot();
        if (fingers.Count == 0) ignoredFingers.Clear();
        if (run != null) Follow(fingers);
    }

    private void Start(Section s, TouchPoint f)
    {
        run = new Run
        {
            Section = s, Action = Settings[s].Slide, FingerId = f.Id,
            Origin = Geometry.Along(s, f.Current),
        };
        if (run.Action == SlideAction.Brightness) brightness.Refresh();
        Log.Debug($"slide start {s} -> {run.Action}");
    }

    private void Follow(IReadOnlyList<TouchPoint> fingers)
    {
        if (run == null) return;
        if (fingers.Count > 1) { Stop(); return; }
        var match = fingers.Where(x => x.Id == run.FingerId).ToList();
        if (match.Count == 0) { Stop(); return; }
        var p = match[0].Current;
        if (!Geometry.InCorridor(run.Section, p, Settings))
        {
            ignoredFingers.Add(run.FingerId);
            Stop();
            return;
        }

        float travel = Geometry.Along(run.Section, p) - run.Origin;
        int target = (int)(travel / StepSize(run.Action));
        int delta = target - run.Steps;
        if (delta == 0) return;
        run.Steps = target;
        Perform(run.Action, delta);
    }

    private float StepSize(SlideAction a) => 0.042f;

    private void Perform(SlideAction action, int delta)
    {
        int n = Math.Abs(delta);
        bool forward = delta > 0;
        switch (action)
        {
            case SlideAction.Volume:
                Post(() => SystemKeys.Tap(forward ? SystemKeys.VolumeUp : SystemKeys.VolumeDown, n * 2));
                break;
            case SlideAction.Brightness:
                Post(() => brightness.Nudge(delta * 5));
                break;
            case SlideAction.Seek:
                Post(() => SystemKeys.Tap(forward ? SystemKeys.Right : SystemKeys.Left, n));
                break;
        }
    }

    private void Stop()
    {
        if (run == null) return;
        Log.Debug($"slide end {run.Section} after {run.Steps} steps");
        run = null;
    }

    // ---------------- clicks (from the mouse hook) ----------------

    /// Returns true when the click belongs to Margin and must be hidden from apps.
    public bool MouseDown()
    {
        if (holdingClick)
        {
            Log.Info("button-up was lost, clearing stale click");
            holdingClick = false;
        }
        if (!Settings.Enabled) return false;

        // A click during a slide is part of the gesture.
        if (run != null)
        {
            holdingClick = true;
            return true;
        }

        // Tap-to-click has no button bit and comes after the finger lifts, so it never qualifies.
        if (touches.DeviceHasButton && !touches.ButtonDown) return false;
        var fingers = touches.Snapshot();
        if (fingers.Count != 1) return false;
        var f = fingers[0];
        if (ignoredFingers.Contains(f.Id)) return false;
        if (Geometry.Hit(f.Start, Settings) is not { } s || Geometry.Hit(f.Current, Settings) != s) return false;
        if (!Geometry.IsAssigned(s, Settings)) return false;

        Start(s, f);
        holdingClick = true;
        return true;
    }

    public void MouseUp()
    {
        if (!holdingClick) return;
        holdingClick = false;
        Stop();
    }

    public void Reset()
    {
        Stop();
        holdingClick = false;
    }
}
