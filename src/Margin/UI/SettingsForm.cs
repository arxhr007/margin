using Margin.Input;
using Margin.Touchpad;

namespace Margin.UI;

/// The settings window: pick a section on the touchpad map, then choose what it does.
/// Every change applies immediately and is saved.
internal sealed class SettingsForm : Form
{
    private readonly Settings settings;
    private readonly Action changed;
    private readonly PadView pad;
    private readonly GroupBox detailBox;
    private readonly FlowLayoutPanel detail;

    public SettingsForm(Settings settings, TouchTracker tracker, Icon icon, Action changed)
    {
        this.settings = settings;
        this.changed = changed;

        Text = "Margin — touchpad edges and corners";
        Icon = icon;
        SuspendLayout();
        // Everything below is laid out in 96-DPI units; WinForms scales it to the screen on load.
        AutoScaleDimensions = new SizeF(96F, 96F);
        AutoScaleMode = AutoScaleMode.Dpi;
        Font = new Font("Segoe UI", 9F);
        StartPosition = FormStartPosition.CenterScreen;
        ClientSize = new Size(900, 580);
        MinimumSize = new Size(780, 520);

        var root = new TableLayoutPanel { Dock = DockStyle.Fill, ColumnCount = 2, RowCount = 3, Padding = new Padding(12) };
        root.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        root.ColumnStyles.Add(new ColumnStyle(SizeType.Absolute, 330));
        root.RowStyles.Add(new RowStyle(SizeType.AutoSize));
        root.RowStyles.Add(new RowStyle(SizeType.Percent, 100));
        root.RowStyles.Add(new RowStyle(SizeType.AutoSize));

        // ---- header: global options ----
        var header = new FlowLayoutPanel { AutoSize = true, Dock = DockStyle.Fill, WrapContents = true, Margin = new Padding(0, 0, 0, 8) };
        var enabled = new CheckBox { Text = "Margin is on", AutoSize = true, Checked = settings.Enabled, Margin = new Padding(0, 6, 18, 0) };
        var activation = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Width = 300 };
        activation.Items.AddRange(new object[] { "Press the edge until it clicks, then slide", "Slide in from the edge (no click)" });
        activation.SelectedIndex = settings.Activation == Activation.SlideIn ? 1 : 0;
        var speed = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Width = 100 };
        speed.Items.AddRange(new object[] { "Slow", "Medium", "Fast" });
        speed.SelectedIndex = (int)settings.Speed;
        var startup = new CheckBox { Text = "Start with Windows", AutoSize = true, Checked = Startup.Enabled, Margin = new Padding(18, 6, 0, 0) };
        header.Controls.AddRange(new Control[]
        {
            enabled, Caption("Start a slide by:"), activation, Caption("Speed:"), speed, startup,
        });
        root.Controls.Add(header, 0, 0);
        root.SetColumnSpan(header, 2);

        // ---- middle: touchpad map + section editor ----
        pad = new PadView(settings, tracker) { Dock = DockStyle.Fill, Margin = new Padding(0, 0, 10, 0) };
        pad.SectionClicked += ShowSection;
        root.Controls.Add(pad, 0, 1);

        detailBox = new GroupBox { Dock = DockStyle.Fill, Padding = new Padding(10) };
        detail = new FlowLayoutPanel { Dock = DockStyle.Fill, FlowDirection = FlowDirection.TopDown, WrapContents = false, AutoScroll = true };
        detailBox.Controls.Add(detail);
        root.Controls.Add(detailBox, 1, 1);

        // ---- footer: sizes and tip ----
        var footer = new TableLayoutPanel { AutoSize = true, Dock = DockStyle.Fill, ColumnCount = 5, RowCount = 2, Margin = new Padding(0, 8, 0, 0) };
        for (int i = 0; i < 4; i++) footer.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        footer.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        var edgeSize = new TrackBar { Minimum = 5, Maximum = 25, TickFrequency = 5, Width = 170, Value = Pct(settings.EdgeDepth, 5, 25) };
        var cornerSize = new TrackBar { Minimum = 8, Maximum = 30, TickFrequency = 2, Width = 170, Value = Pct(settings.CornerSize, 8, 30) };
        footer.Controls.Add(Caption("Edge size"), 0, 0);
        footer.Controls.Add(edgeSize, 1, 0);
        footer.Controls.Add(Caption("Corner size"), 2, 0);
        footer.Controls.Add(cornerSize, 3, 0);
        var tip = new Label
        {
            AutoSize = true,
            ForeColor = SystemColors.GrayText,
            Text = "Click a section on the touchpad to change it. Orange dots show your fingers. Hold Ctrl + Shift while sliding for fine steps.",
        };
        footer.Controls.Add(tip, 0, 1);
        footer.SetColumnSpan(tip, 5);
        root.Controls.Add(footer, 0, 2);
        root.SetColumnSpan(footer, 2);

        Controls.Add(root);

        enabled.CheckedChanged += (_, _) => { settings.Enabled = enabled.Checked; Changed(); };
        activation.SelectedIndexChanged += (_, _) =>
        {
            settings.Activation = activation.SelectedIndex == 1 ? Activation.SlideIn : Activation.Press;
            Changed();
            ShowSection(pad.Selected); // refresh the how-to text
        };
        speed.SelectedIndexChanged += (_, _) => { settings.Speed = (Speed)speed.SelectedIndex; Changed(); };
        startup.CheckedChanged += (_, _) => Startup.Enabled = startup.Checked;
        edgeSize.ValueChanged += (_, _) => { settings.EdgeDepth = edgeSize.Value / 100.0; Changed(); };
        cornerSize.ValueChanged += (_, _) => { settings.CornerSize = cornerSize.Value / 100.0; Changed(); };

        ResumeLayout(false);
        PerformLayout();
    }

    // Build the section editor after scaling, so it can size itself to the real panel width.
    protected override void OnLoad(EventArgs e)
    {
        base.OnLoad(e);
        ShowSection(Section.Right);
    }

    private static int Pct(double fraction, int min, int max) => Math.Clamp((int)Math.Round(fraction * 100), min, max);

    private static Label Caption(string text) => new() { Text = text, AutoSize = true, Margin = new Padding(8, 7, 4, 0) };

    private void Changed()
    {
        pad.Invalidate();
        changed();
    }

    private void ShowSection(Section s)
    {
        pad.Selected = s;
        pad.Invalidate();
        detailBox.Text = s.Name();

        detail.SuspendLayout();
        foreach (Control c in detail.Controls.Cast<Control>().ToList()) c.Dispose();
        detail.Controls.Clear();
        int width = detail.ClientSize.Width - SystemInformation.VerticalScrollBarWidth - 6;

        if (s.IsCorner()) BuildCorner(s, width);
        else BuildEdge(s, width);

        detail.ResumeLayout();
    }

    private void BuildEdge(Section s, int width)
    {
        var v = settings[s];
        detail.Controls.Add(Para("Sliding along this edge controls:", width, bold: true));
        var combo = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Width = width, Margin = new Padding(0, 4, 0, 12) };
        foreach (var a in Enum.GetValues<SlideAction>()) combo.Items.Add(Labels.Long(a));
        combo.SelectedIndex = (int)v.Slide;
        combo.SelectedIndexChanged += (_, _) =>
        {
            v.Slide = (SlideAction)combo.SelectedIndex;
            if (v.Slide == SlideAction.Custom && string.IsNullOrWhiteSpace(v.Forward) && string.IsNullOrWhiteSpace(v.Back))
                (v.Forward, v.Back, v.HoldModifiers) = ("Ctrl+Tab", "Ctrl+Shift+Tab", false);
            Changed();
            BeginInvoke(() => ShowSection(s)); // rebuild so the key fields appear or disappear
        };
        detail.Controls.Add(combo);
        if (v.Slide == SlideAction.Custom) BuildCustomKeys(v, s, width);

        var direction = s.IsVertical()
            ? "Slide up for more / next, down for less / previous."
            : "Slide right for forward / next, left for back / previous.";
        var how = settings.Activation == Activation.Press
            ? "Rest a finger on the edge, press until the touchpad clicks, and slide without letting go."
            : "Bring your finger in from the laptop body onto this edge, then slide along it. No click needed.";
        detail.Controls.Add(Para(direction, width));
        detail.Controls.Add(Para(how, width, gray: true));
    }

    private static readonly (string Name, string Forward, string Back, bool Hold)[] KeyPresets =
    {
        ("Alt+Tab: switch windows", "Alt+Tab", "Alt+Shift+Tab", true),
        ("Ctrl+Tab: switch browser tabs", "Ctrl+Tab", "Ctrl+Shift+Tab", false),
        ("Ctrl+PageDown / PageUp: next / previous tab", "Ctrl+PageDown", "Ctrl+PageUp", false),
        ("Page Up / Page Down: scroll", "PageUp", "PageDown", false),
        ("Right / Left arrow", "Right", "Left", false),
        ("Ctrl+Y / Ctrl+Z: redo / undo", "Ctrl+Y", "Ctrl+Z", false),
    };

    /// Lets the user choose which shortcut each slide direction sends.
    private void BuildCustomKeys(SectionSettings v, Section s, int width)
    {
        var (fwdName, backName) = s.IsVertical() ? ("Slide up", "Slide down") : ("Slide right", "Slide left");

        detail.Controls.Add(Para("Preset:", width, margin: new Padding(0, 4, 0, 2)));
        var presets = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Width = width, Margin = new Padding(0, 0, 0, 8) };
        presets.Items.Add("Choose a preset…");
        foreach (var p in KeyPresets) presets.Items.Add(p.Name);
        presets.SelectedIndex = 0;
        presets.SelectedIndexChanged += (_, _) =>
        {
            if (presets.SelectedIndex <= 0) return;
            var p = KeyPresets[presets.SelectedIndex - 1];
            (v.Forward, v.Back, v.HoldModifiers) = (p.Forward, p.Back, p.Hold);
            Changed();
            BeginInvoke(() => ShowSection(s));
        };
        detail.Controls.Add(presets);

        AddKeyRow($"{fwdName} presses:", v.Forward, text => v.Forward = text, width);
        AddKeyRow($"{backName} presses:", v.Back, text => v.Back = text, width);

        var hold = new CheckBox
        {
            Text = "Hold shared modifiers while sliding",
            AutoSize = true,
            Checked = v.HoldModifiers,
            Margin = new Padding(0, 6, 0, 6),
        };
        hold.CheckedChanged += (_, _) => { v.HoldModifiers = hold.Checked; Changed(); };
        detail.Controls.Add(hold);

        detail.Controls.Add(Para(
            "Click Record, then press the keys. Windows keeps a few combos (Alt+Tab, Win+…) to itself, so type those instead, " +
            "like Alt+Tab or Win+Ctrl+Right. Each step of the slide presses the shortcut once. " +
            "\"Hold shared modifiers\" keeps Alt down until you lift your finger, like holding Alt while pressing Tab.", width, gray: true));
    }

    private void AddKeyRow(string caption, string? current, Action<string?> save, int width)
    {
        detail.Controls.Add(Para(caption, width, margin: new Padding(0, 6, 0, 2)));
        var row = new FlowLayoutPanel { AutoSize = true, WrapContents = false, Margin = new Padding(0) };
        var box = new ShortcutBox { Width = width - LogicalToDeviceUnits(84), Text = current ?? "" };
        var record = new Button { Text = "Record", AutoSize = true, AutoSizeMode = AutoSizeMode.GrowAndShrink, Margin = new Padding(6, 0, 0, 0) };
        row.Controls.Add(box);
        row.Controls.Add(record);

        void Validate()
        {
            bool bad = box.Text.Length > 0 && !ActionRunner.TryParseShortcut(box.Text, out _, out _);
            box.BackColor = bad ? Color.FromArgb(255, 228, 228) : SystemColors.Window;
        }

        box.TextChanged += (_, _) =>
        {
            save(box.Text.Length > 0 ? box.Text : null);
            Validate();
            Changed();
        };
        record.Click += (_, _) => { record.Text = "Press keys…"; box.BeginRecording(); };
        box.RecordingEnded += (_, _) => record.Text = "Record";
        Validate();
        detail.Controls.Add(row);
    }

    private void BuildCorner(Section s, int width)
    {
        var clicks = settings[s].Clicks;
        detail.Controls.Add(Para("Press this corner until the touchpad clicks:", width, bold: true));

        for (int i = 0; i < 3; i++)
        {
            int index = i;
            detail.Controls.Add(Para(index == 0 ? "1 click" : $"{index + 1} clicks", width, margin: new Padding(0, 10, 0, 2)));

            var current = index < clicks.Count ? clicks[index] : new ClickSpec();
            var combo = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Width = width };
            foreach (var a in Enum.GetValues<ClickAction>()) combo.Items.Add(Labels.Long(a));
            combo.SelectedIndex = (int)current.Do;

            var row = new FlowLayoutPanel { AutoSize = true, WrapContents = false, Margin = new Padding(0, 4, 0, 0) };
            var value = new TextBox { Width = width - LogicalToDeviceUnits(80), Text = current.Value ?? "" };
            var browse = new Button { Text = "Browse…", AutoSize = true, Margin = new Padding(6, 0, 0, 0) };
            row.Controls.Add(value);
            row.Controls.Add(browse);

            void Sync()
            {
                var a = (ClickAction)combo.SelectedIndex;
                row.Visible = Labels.NeedsValue(a);
                value.PlaceholderText = Labels.Placeholder(a);
                value.Width = a == ClickAction.App ? width - LogicalToDeviceUnits(80) : width;
                browse.Visible = a == ClickAction.App;
                bool bad = a == ClickAction.Keys && value.Text.Length > 0 && !ActionRunner.TryParseShortcut(value.Text, out _, out _);
                value.BackColor = bad ? Color.FromArgb(255, 228, 228) : SystemColors.Window;
            }

            ClickSpec Slot()
            {
                while (clicks.Count <= index) clicks.Add(new ClickSpec());
                return clicks[index];
            }

            combo.SelectedIndexChanged += (_, _) =>
            {
                var spec = Slot();
                spec.Do = (ClickAction)combo.SelectedIndex;
                if (!Labels.NeedsValue(spec.Do)) spec.Value = null;
                else if (value.Text.Length > 0) spec.Value = value.Text;
                Sync();
                Changed();
            };
            value.TextChanged += (_, _) =>
            {
                Slot().Value = value.Text.Length > 0 ? value.Text : null;
                Sync();
                Changed();
            };
            browse.Click += (_, _) =>
            {
                using var dlg = new OpenFileDialog { Filter = "Programs|*.exe;*.lnk|All files|*.*", Title = "Choose an app" };
                if (dlg.ShowDialog(this) == DialogResult.OK) value.Text = dlg.FileName;
            };

            detail.Controls.Add(combo);
            detail.Controls.Add(row);
            Sync();
        }

        detail.Controls.Add(Para("Corners react only to a real press, never to a tap. Unused corners stay normal touchpad.", width,
            gray: true, margin: new Padding(0, 14, 0, 0)));
    }

    private static Label Para(string text, int width, bool bold = false, bool gray = false, Padding? margin = null)
    {
        var l = new Label
        {
            Text = text,
            AutoSize = true,
            MaximumSize = new Size(width, 0),
            Margin = margin ?? new Padding(0, 0, 0, 6),
        };
        if (bold) l.Font = new Font(SystemFonts.MessageBoxFont ?? l.Font, FontStyle.Bold);
        if (gray) l.ForeColor = SystemColors.GrayText;
        return l;
    }
}

/// A text box that can also record a shortcut: click Record, press the keys, and it fills in e.g. "Ctrl+Shift+Tab".
internal sealed class ShortcutBox : TextBox
{
    private bool recording;

    public event EventHandler? RecordingEnded;

    public void BeginRecording()
    {
        recording = true;
        Focus();
    }

    private void EndRecording()
    {
        if (!recording) return;
        recording = false;
        RecordingEnded?.Invoke(this, EventArgs.Empty);
    }

    protected override void OnLostFocus(EventArgs e)
    {
        EndRecording();
        base.OnLostFocus(e);
    }

    protected override bool IsInputKey(Keys keyData) => recording || base.IsInputKey(keyData);

    protected override bool ProcessCmdKey(ref Message msg, Keys keyData)
    {
        if (!recording) return base.ProcessCmdKey(ref msg, keyData);

        var key = keyData & Keys.KeyCode;
        if (key is Keys.ControlKey or Keys.ShiftKey or Keys.Menu or Keys.LWin or Keys.RWin) return true; // wait for the real key
        if (KeyMap.Name(key) is not { } name) return true; // not bindable: keep waiting

        var parts = new List<string>();
        if ((keyData & Keys.Control) != 0) parts.Add("Ctrl");
        if ((keyData & Keys.Alt) != 0) parts.Add("Alt");
        if ((keyData & Keys.Shift) != 0) parts.Add("Shift");
        if ((Native.GetAsyncKeyState(0x5B) & 0x8000) != 0 || (Native.GetAsyncKeyState(0x5C) & 0x8000) != 0) parts.Add("Win");
        parts.Add(name switch
        {
            "pageup" => "PageUp", "pagedown" => "PageDown", "printscreen" => "PrintScreen", "volumeup" => "VolumeUp",
            "volumedown" => "VolumeDown", "playpause" => "PlayPause", "nexttrack" => "NextTrack", "prevtrack" => "PrevTrack",
            _ when name.Length == 1 => name.ToUpperInvariant(),
            _ => char.ToUpperInvariant(name[0]) + name[1..],
        });
        Text = string.Join("+", parts);
        EndRecording();
        return true;
    }
}

/// "Start with Windows" via the per-user Run key.
internal static class Startup
{
    private const string RunKey = @"Software\Microsoft\Windows\CurrentVersion\Run";
    private const string Name = "Margin";

    public static bool Enabled
    {
        get
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(RunKey);
            return key?.GetValue(Name) is string;
        }
        set
        {
            using var key = Microsoft.Win32.Registry.CurrentUser.CreateSubKey(RunKey);
            if (value) key.SetValue(Name, $"\"{Environment.ProcessPath}\"");
            else key.DeleteValue(Name, throwOnMissingValue: false);
        }
    }
}
