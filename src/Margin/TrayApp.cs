using System.Diagnostics;
using System.Drawing.Drawing2D;
using Margin.Input;
using Margin.Touchpad;

namespace Margin;

/// Owns everything: touchpad input, mouse hook, engine and the tray icon.
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

    public TrayApp()
    {
        touchpad = new RawTouchpad(tracker);
        engine = new GestureEngine(tracker, hud, brightness) { Settings = settings };
        hook = new MouseHook(touchpad, engine);

        icon = MakeIcon();
        enabledItem = new ToolStripMenuItem("On", null, (_, _) =>
        {
            settings.Enabled = !settings.Enabled;
            SettingsStore.Save(settings);
        });
        var menu = new ContextMenuStrip();
        menu.Items.Add(enabledItem);
        menu.Items.Add("Edit settings file…", null, (_, _) => Process.Start(new ProcessStartInfo("notepad.exe") { ArgumentList = { SettingsStore.FilePath } })?.Dispose());
        menu.Items.Add(new ToolStripSeparator());
        menu.Items.Add("Open log", null, (_, _) => Process.Start(new ProcessStartInfo("notepad.exe") { ArgumentList = { Log.FilePath } })?.Dispose());
        menu.Items.Add("Quit Margin", null, (_, _) => ExitThread());
        menu.Opening += (_, _) => enabledItem.Checked = settings.Enabled;

        tray = new NotifyIcon { Icon = icon, Text = "Margin", ContextMenuStrip = menu, Visible = true };

        Log.Info($"Margin {Application.ProductVersion} started");
    }

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
        SettingsStore.Save(settings);
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
