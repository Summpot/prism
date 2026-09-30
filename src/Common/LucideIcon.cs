using System;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;

namespace Prism.Common;

/// <summary>
/// Renders Lucide/Feather stroke-based vector icons with precise stroke thickness,
/// rounded caps and joins, and automatic foreground color inheritance.
/// </summary>
public class LucideIcon : Control
{
    public static readonly StyledProperty<Geometry?> DataProperty =
        AvaloniaProperty.Register<LucideIcon, Geometry?>(nameof(Data));

    public static readonly StyledProperty<IBrush?> ForegroundProperty =
        AvaloniaProperty.Register<LucideIcon, IBrush?>(nameof(Foreground), inherits: true);

    public static readonly StyledProperty<double> StrokeThicknessProperty =
        AvaloniaProperty.Register<LucideIcon, double>(nameof(StrokeThickness), 1.8);

    public Geometry? Data
    {
        get => GetValue(DataProperty);
        set => SetValue(DataProperty, value);
    }

    public IBrush? Foreground
    {
        get => GetValue(ForegroundProperty);
        set => SetValue(ForegroundProperty, value);
    }

    public double StrokeThickness
    {
        get => GetValue(StrokeThicknessProperty);
        set => SetValue(StrokeThicknessProperty, value);
    }

    static LucideIcon()
    {
        AffectsRender<LucideIcon>(DataProperty, ForegroundProperty, StrokeThicknessProperty);
    }

    public override void Render(DrawingContext context)
    {
        if (Data == null) return;

        var brush = Foreground ?? Brushes.Black;
        var pen = new Pen(brush, StrokeThickness, lineCap: PenLineCap.Round, lineJoin: PenLineJoin.Round);

        // Standard Lucide icons use a 24x24 viewBox
        var bounds = Data.Bounds;
        double srcWidth = Math.Max(24.0, bounds.Width + bounds.X);
        double srcHeight = Math.Max(24.0, bounds.Height + bounds.Y);

        if (srcWidth <= 0 || srcHeight <= 0) return;

        double scaleX = Bounds.Width / srcWidth;
        double scaleY = Bounds.Height / srcHeight;
        double scale = Math.Min(scaleX, scaleY);

        double offsetX = (Bounds.Width - srcWidth * scale) / 2.0;
        double offsetY = (Bounds.Height - srcHeight * scale) / 2.0;

        using (context.PushTransform(Matrix.CreateTranslation(offsetX, offsetY) * Matrix.CreateScale(scale, scale)))
        {
            context.DrawGeometry(null, pen, Data);
        }
    }
}
