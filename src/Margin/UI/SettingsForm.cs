using Margin.Touchpad;

namespace Margin.UI;

/// The settings window: pick an edge on the touchpad map, then choose what it does.
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

        Text = "Margin — touchpad edges";
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
        var footer = new TableLayoutPanel { AutoSize = true, Dock = DockStyle.Fill, ColumnCount = 3, RowCount = 2, Margin = new Padding(0, 8, 0, 0) };
        for (int i = 0; i < 2; i++) footer.ColumnStyles.Add(new ColumnStyle(SizeType.AutoSize));
        footer.ColumnStyles.Add(new ColumnStyle(SizeType.Percent, 100));
        var edgeSize = new TrackBar { Minimum = 5, Maximum = 25, TickFrequency = 5, Width = 170, Value = Pct(settings.EdgeDepth, 5, 25) };
        footer.Controls.Add(Caption("Edge size"), 0, 0);
        footer.Controls.Add(edgeSize, 1, 0);
        var tip = new Label
        {
            AutoSize = true,
            ForeColor = SystemColors.GrayText,
            Text = "Click an edge on the touchpad to change it. Orange dots show your fingers.",
        };
        footer.Controls.Add(tip, 0, 1);
        footer.SetColumnSpan(tip, 3);
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

        var v = settings[s];
        detail.Controls.Add(Para("Sliding along this edge controls:", width, bold: true));
        var combo = new ComboBox { DropDownStyle = ComboBoxStyle.DropDownList, Width = width, Margin = new Padding(0, 4, 0, 12) };
        foreach (var a in Enum.GetValues<SlideAction>()) combo.Items.Add(Labels.Long(a));
        combo.SelectedIndex = (int)v.Slide;
        combo.SelectedIndexChanged += (_, _) => { v.Slide = (SlideAction)combo.SelectedIndex; Changed(); };
        detail.Controls.Add(combo);

        var direction = s.IsVertical()
            ? "Slide up for more / next, down for less / previous."
            : "Slide right for forward / next, left for back / previous.";
        var how = settings.Activation == Activation.Press
            ? "Rest a finger on the edge, press until the touchpad clicks, and slide without letting go."
            : "Bring your finger in from the laptop body onto this edge, then slide along it. No click needed.";
        detail.Controls.Add(Para(direction, width));
        detail.Controls.Add(Para(how, width, gray: true));

        detail.ResumeLayout();
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
