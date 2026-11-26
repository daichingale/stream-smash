using System.Drawing;
using System.Windows.Forms;

static class StudioColors
{
    public static readonly Color Background = Color.FromArgb(240, 244, 250);
    public static readonly Color Panel = Color.FromArgb(251, 253, 255);
    static readonly Color Field = Color.White;
    static readonly Color Text = Color.FromArgb(24, 37, 59);
    static readonly Color Border = Color.FromArgb(153, 170, 192);
    public static readonly Color Accent = Color.FromArgb(22, 73, 162);

    public static void Apply(Control control)
    {
        control.ForeColor = Text;
        if (control is Button) {
            var button = (Button)control;
            button.FlatStyle = FlatStyle.Flat;
            button.BackColor = Field;
            button.FlatAppearance.BorderColor = Border;
            button.Padding = new Padding(5, 2, 5, 2);
        } else if (control is TextBox || control is ComboBox || control is NumericUpDown) {
            control.BackColor = Field;
            if (control is ComboBox) ((ComboBox)control).FlatStyle = FlatStyle.Standard;
            if (control is NumericUpDown) {
                foreach (Control child in control.Controls) {
                    child.BackColor = Field;
                    child.ForeColor = Text;
                }
            }
        }
        foreach (Control child in control.Controls) Apply(child);
    }

    public static void Primary(Button button)
    {
        button.BackColor = Accent;
        button.ForeColor = Color.White;
        button.FlatAppearance.BorderColor = Accent;
    }
}
