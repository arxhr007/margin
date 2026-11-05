using System.Diagnostics;
using System.Drawing.Drawing2D;
using Margin.Input;
using Margin.Touchpad;
using Margin.UI;
using Microsoft.Win32;

namespace Margin;

/// Owns everything: touchpad input, mouse hook, engine, tray icon and the settings window.
internal sealed class TrayApp : ApplicationContext
{
    private readonly Settings settings = SettingsStore.Load();
    private readonly TouchTracker tracker = new();
    private readonly Hud hud = new();
    private readonly Brightness brightness = new();
    private readonly RawTouchpad touchpad;
    private readonly GestureEngine engine;
    private readonly MouseHook hook;
    private readonly NotifyIcon tray;
    private readonly Icon icon;
    private readonly ToolStripMenuItem enabledItem;
    private readonly System.Windows.Forms.Timer saveTimer = new() { Interval = 400 };
    private SettingsForm? window;

    public TrayApp(bool firstRun, EventWaitHandle showSignal)
    {
        // Launching Margin a second time opens this copy's settings.
        ThreadPool.RegisterWaitForSingleObject(showSignal, (_, _) => hud.BeginInvoke(ShowSettings), null, Timeout.Infinite, executeOnlyOnce: false);
        touchpad = new RawTouchpad(tracker);
        engine = new GestureEngine(tracker, hud, brightness) { Settings = settings };
        hook = new MouseHook(touchpad, engine);
        saveTimer.Tick += (_, _) => { saveTimer.Stop(); SettingsStore.Save(settings); };

        icon = MakeIcon();
        enabledItem = new ToolStripMenuItem("On", null, (_, _) => { settings.Enabled = !settings.Enabled; SettingsChanged(); });
        var menu = new ContextMenuStrip();
        menu.Items.Add(new ToolStripMenuItem("Settings…", null, (_, _) => ShowSettings()) { Font = new Font(menu.Font, FontStyle.Bold) });
        menu.Items.Add(enabledItem);
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add("Open log", null, (_, _) => Process.Start(new ProcessStartInfo("notepad.exe") { ArgumentList = { Log.FilePath } })?.Dispose());
        menu.Items.Add("Quit Margin", null, (_, _) => ExitThread());
        menu.Opening += (_, _) => enabledItem.Checked = settings.Enabled;

        tray = new NotifyIcon { Icon = icon, Text = "Margin", ContextMenuStrip = menu, Visible = true };
        tray.MouseClick += (_, e) => { if (e.Button == MouseButtons.Left) ShowSettings(); };

        SystemEvents.PowerModeChanged += OnPowerModeChanged;
        SystemEvents.SessionSwitch += OnSessionSwitch;

        Log.Info($"Margin {Application.ProductVersion} started");
        if (firstRun)
        {
            SettingsStore.Save(settings);
            ShowSettings();
        }
    }

    private void SettingsChanged()
    {
        engine.Reset();
        tray.Text = settings.Enabled ? "Margin" : "Margin (off)";
        saveTimer.Stop();
        saveTimer.Start();
    }

    private void ShowSettings()
    {
        if (window == null || window.IsDisposed)
        {
            window = new SettingsForm(settings, tracker, icon, SettingsChanged);
            window.FormClosed += (_, _) => window = null;
            window.Show();
        }
        if (window.WindowState == FormWindowState.Minimized) window.WindowState = FormWindowState.Normal;
        window.Activate();
    }

    private void OnPowerModeChanged(object? sender, PowerModeChangedEventArgs e)
    {
        if (e.Mode == PowerModes.Resume) Rearm("resume");
    }

    private void OnSessionSwitch(object? sender, SessionSwitchEventArgs e)
    {
        if (e.Reason is SessionSwitchReason.SessionUnlock or SessionSwitchReason.ConsoleConnect) Rearm("unlock");
    }

    /// The hook and raw input registration can be dropped across sleep or the lock screen.
    private void Rearm(string why) => hud.BeginInvoke(() =>
    {
        Log.Info($"re-arming input after {why}");
        engine.Reset();
        tracker.Clear();
        touchpad.Register();
        hook.Install();
    });

    /// A touchpad outline with highlighted edges.
    private static Icon MakeIcon()
    {
        int s = Math.Max(16, SystemInformation.SmallIconSize.Width);
        using var bmp = new Bitmap(s, s);
        using (var g = Graphics.FromImage(bmp))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            float u = s / 16f;
            var body = new RectangleF(1.5f * u, 3f * u, 13f * u, 10f * u);
            using var path = new GraphicsPath();
            float d = 3f * u;
            path.AddArc(body.Left, body.Top, d, d, 180, 90);
            path.AddArc(body.Right - d, body.Top, d, d, 270, 90);
            path.AddArc(body.Right - d, body.Bottom - d, d, d, 0, 90);
            path.AddArc(body.Left, body.Bottom - d, d, d, 90, 90);
            path.CloseFigure();
            using var pen = new Pen(Color.White, 1.2f * u);
            g.DrawPath(pen, path);
            using var accent = new SolidBrush(Color.FromArgb(255, 255, 160, 40));
            g.FillRectangle(accent, body.Left + 1.6f * u, body.Top + 2.2f * u, 1.5f * u, 5.6f * u);
            g.FillRectangle(accent, body.Right - 3.1f * u, body.Top + 2.2f * u, 1.5f * u, 5.6f * u);
        }
        var handle = bmp.GetHicon();
        var result = (Icon)Icon.FromHandle(handle).Clone();
        Native.DestroyIcon(handle);
        return result;
    }

    protected override void ExitThreadCore()
    {
        SystemEvents.PowerModeChanged -= OnPowerModeChanged;
        SystemEvents.SessionSwitch -= OnSessionSwitch;
        if (saveTimer.Enabled) SettingsStore.Save(settings);
        window?.Close();
        engine.Reset();
        hook.Dispose();
        touchpad.Dispose();
        brightness.Dispose();
        tray.Visible = false;
        tray.Dispose();
        icon.Dispose();
        hud.Dispose();
        Log.Info("Margin quit");
        base.ExitThreadCore();
    }
}
