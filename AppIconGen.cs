using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Text;

class Program
{
    static void Main()
    {
        int size = 256;
        using (Bitmap bmp = new Bitmap(size, size, System.Drawing.Imaging.PixelFormat.Format32bppArgb))
        using (Graphics g = Graphics.FromImage(bmp))
        {
            g.SmoothingMode = SmoothingMode.AntiAlias;
            g.TextRenderingHint = TextRenderingHint.AntiAliasGridFit;
            g.Clear(Color.Transparent);

            // Draw a rounded rectangle background
            int padding = size / 16;
            Rectangle rect = new Rectangle(padding, padding, size - padding * 2, size - padding * 2);

            using (GraphicsPath path = new GraphicsPath())
            {
                int radius = size / 4;
                path.AddArc(rect.X, rect.Y, radius, radius, 180, 90);
                path.AddArc(rect.Right - radius, rect.Y, radius, radius, 270, 90);
                path.AddArc(rect.Right - radius, rect.Bottom - radius, radius, radius, 0, 90);
                path.AddArc(rect.X, rect.Bottom - radius, radius, radius, 90, 90);
                path.CloseFigure();

                using (LinearGradientBrush brush = new LinearGradientBrush(rect, Color.FromArgb(255, 60, 120, 210), Color.FromArgb(255, 30, 80, 150), LinearGradientMode.ForwardDiagonal))
                {
                    g.FillPath(brush, path);
                }
            }

            // Draw text "한"
            string text = "한";
            Font font = new Font("Malgun Gothic", size * 0.4f, FontStyle.Bold);
            
            StringFormat sf = new StringFormat();
            sf.Alignment = StringAlignment.Center;
            sf.LineAlignment = StringAlignment.Center;

            g.DrawString(text, font, Brushes.White, rect, sf);

            IntPtr hIcon = bmp.GetHicon();
            using (System.IO.FileStream fs = new System.IO.FileStream("app.ico", System.IO.FileMode.Create))
            {
                Icon.FromHandle(hIcon).Save(fs);
            }
        }
    }
}
