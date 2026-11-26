using System.Drawing;
using System.Windows.Forms;

static class StudioColors
{
    public static readonly Color Background = Color.FromArgb(18, 24, 35);
    public static readonly Color Panel = Color.FromArgb(27, 36, 51);
    static readonly Color Field = Color.FromArgb(39, 51, 70);
    static readonly Color Text = Color.FromArgb(232, 239, 249);
    static readonly Color Border = Color.FromArgb(70, 86, 109);
    public static readonly Color Accent = Color.FromArgb(106, 235, 207);

    public static void Apply(Control control)
    {
        if (control.ForeColor == SystemColors.ControlText) control.ForeColor = Text;
        if (control is Button) {
            var button = (Button)control;
            button.FlatStyle = FlatStyle.Flat;
            button.BackColor = Field;
            button.FlatAppearance.BorderColor = Border;
            button.Padding = new Padding(5, 2, 5, 2);
        } else if (control is TextBox || control is ComboBox || control is NumericUpDown) {
            control.BackColor = Field;
            if (control is ComboBox) ((ComboBox)control).FlatStyle = FlatStyle.Flat;
        }
        foreach (Control child in control.Controls) Apply(child);
    }

    public static void Primary(Button button)
    {
        button.BackColor = Accent;
        button.ForeColor = Background;
        button.FlatAppearance.BorderColor = Accent;
    }
}
