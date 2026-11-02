using System.Drawing;
using Margin.Input;
using Margin.Touchpad;

namespace Margin;

/// Turns touchpad frames and clicks into edge actions.
///
/// Two ways to start (user's choice):
///   Press   — one finger rests in an assigned edge, the touchpad is physically clicked, then the finger
///             slides while held. The click and the drag are hidden from apps.
///   SlideIn — one finger first appears in the thin outer strip of an assigned edge (it came in from the
///             laptop body), then moves mostly along the edge within a short window. No click needed.
///             Fingers born anywhere else, or any multi-finger contact, are ignored until all fingers lift.
/// While a slide runs the pointer is frozen and travel along the edge is turned into steps.
internal sealed class GestureEngine
{
    private const long IntentWindowMs = 600;
    private const float IntentTravel = 0.03f;

    private sealed class Run
    {
        public required Section Section;
        public required SlideAction Action;
        public required int FingerId;
        public required float Origin;
        public required bool FromPress;
        public int Steps;
    }

    private sealed class Candidate
    {
        public required Section Section;
        public required int FingerId;
        public required long BornAt;
    }

    private readonly TouchTracker touches;
    private readonly Hud hud;
    private readonly Brightness brightness;
    private readonly SynchronizationContext ui;
    private readonly HashSet<int> ignoredFingers = new();

    private Run? run;
    private Candidate? candidate;
    private bool multiFingerSeen;
    private bool holdingClick;

    public Settings Settings { get; set; } = new();

    /// While true the mouse hook drops pointer movement.
    public bool FreezePointer => run != null;

    public GestureEngine(TouchTracker touches, Hud hud, Brightness brightness)
    {
        this.touches = touches;
        this.hud = hud;
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
        if (fingers.Count == 0)
        {
            multiFingerSeen = false;
            ignoredFingers.Clear();
            candidate = null;
        }
        else if (fingers.Count > 1)
        {
            multiFingerSeen = true;
            candidate = null;
        }

        if (run != null) Follow(fingers);
        else if (Settings.Enabled && Settings.Activation == Activation.SlideIn) Watch(fingers);
    }

    /// Slide-in mode: look for a finger that entered from outside and then moves along the edge.
    private void Watch(IReadOnlyList<TouchPoint> fingers)
    {
        if (fingers.Count != 1 || multiFingerSeen) return;
        var f = fingers[0];
        if (ignoredFingers.Contains(f.Id)) return;
        long now = Environment.TickCount64;

        if (candidate == null || candidate.FingerId != f.Id)
        {
            // Judge each finger once, by where it first touched down.
            if (Geometry.EntryEdge(f.Start, Settings) is { } edge)
                candidate = new Candidate { Section = edge, FingerId = f.Id, BornAt = now };
            else
                ignoredFingers.Add(f.Id);
            return;
        }

        var s = candidate.Section;
        float along = Math.Abs(Geometry.Along(s, f.Current) - Geometry.Along(s, f.Start));
        float inward = Math.Max(0, Geometry.Inward(s, f.Start, f.Current));
        bool tooLate = now - candidate.BornAt > IntentWindowMs;
        if (tooLate || !Geometry.InCorridor(s, f.Current, Settings) || (along >= IntentTravel && along < inward * 0.6f))
        {
            ignoredFingers.Add(f.Id); // wandered inward, went diagonal, or hesitated: it's normal pointer use
            candidate = null;
            return;
        }
        if (along >= IntentTravel)
        {
            candidate = null;
            Start(s, f, fromPress: false);
        }
    }

    private void Start(Section s, TouchPoint f, bool fromPress)
    {
        run = new Run
        {
            Section = s, Action = Settings[s].Slide, FingerId = f.Id,
            Origin = Geometry.Along(s, f.Current), FromPress = fromPress,
        };
        if (run.Action == SlideAction.Brightness) brightness.Refresh();
        Log.Debug($"slide start {s} -> {run.Action} ({(fromPress ? "press" : "slide-in")})");
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
        Perform(run.Action, delta, target);
    }

    private float StepSize(SlideAction a) =>
        // Discrete actions (tabs, tracks, desktops) get bigger steps so one swipe doesn't overshoot.
        a is SlideAction.Tabs or SlideAction.Tracks or SlideAction.Desktops ? 0.105f : 0.042f;

    private void Perform(SlideAction action, int delta, int total)
    {
        int n = Math.Abs(delta);
        bool forward = delta > 0;
        switch (action)
        {
            case SlideAction.Volume:
                Post(() => SystemKeys.Tap(forward ? SystemKeys.VolumeUp : SystemKeys.VolumeDown, n * 2));
                break;
            case SlideAction.Brightness:
                int change = delta * 5;
                Post(() =>
                {
                    int value = brightness.Nudge(change);
                    if (value >= 0) hud.ShowLevel($"Brightness  {value}%", value);
                    else hud.ShowText("Brightness not available");
                });
                break;
            case SlideAction.Seek:
                Post(() =>
                {
                    SystemKeys.Tap(forward ? SystemKeys.Right : SystemKeys.Left, n);
                    hud.ShowText(total >= 0 ? $"⏩  +{total * 5}s" : $"⏪  −{-total * 5}s");
                });
                break;
            case SlideAction.Zoom:
                Post(() => SystemKeys.CtrlWheel(forward ? n : -n));
                break;
            case SlideAction.Tabs:
                Post(() => SystemKeys.Combo(SystemKeys.Tab,
                    forward ? new[] { SystemKeys.Ctrl } : new[] { SystemKeys.Ctrl, SystemKeys.Shift }, n));
                break;
            case SlideAction.Tracks:
                Post(() => SystemKeys.Tap(forward ? SystemKeys.MediaNext : SystemKeys.MediaPrev, n));
                break;
            case SlideAction.Desktops:
                Post(() => SystemKeys.Combo(forward ? SystemKeys.Right : SystemKeys.Left,
                    new[] { SystemKeys.Ctrl, SystemKeys.Win }, n));
                break;
        }
    }

    private void Stop()
    {
        if (run == null) return;
        var action = run.Action;
        Log.Debug($"slide end {run.Section} after {run.Steps} steps");
        run = null;
        if (action is SlideAction.Brightness or SlideAction.Seek) Post(() => hud.HideAfter(700));
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
        if (Settings.Activation != Activation.Press) return false;

        Start(s, f, fromPress: true);
        holdingClick = true;
        return true;
    }

    public void MouseUp()
    {
        if (!holdingClick) return;
        holdingClick = false;
        if (run is { FromPress: true }) Stop();
    }

    public void Reset()
    {
        Stop();
        candidate = null;
        holdingClick = false;
    }
}
