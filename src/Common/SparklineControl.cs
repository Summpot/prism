using System;
using System.Collections.Generic;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Media;

namespace Prism.Common;

public class SparklineControl : Control
{
    public static readonly StyledProperty<IEnumerable<ulong>?> SamplesProperty =
        AvaloniaProperty.Register<SparklineControl, IEnumerable<ulong>?>(nameof(Samples));

    public static readonly StyledProperty<IBrush?> StrokeProperty =
        AvaloniaProperty.Register<SparklineControl, IBrush?>(nameof(Stroke), new SolidColorBrush(Color.Parse("#10b981")));

    public static readonly StyledProperty<double> StrokeThicknessProperty =
        AvaloniaProperty.Register<SparklineControl, double>(nameof(StrokeThickness), 2.0);

    public static readonly StyledProperty<IBrush?> FillProperty =
        AvaloniaProperty.Register<SparklineControl, IBrush?>(nameof(Fill), new SolidColorBrush(Color.Parse("#2010b981")));

    public IEnumerable<ulong>? Samples
    {
        get => GetValue(SamplesProperty);
        set => SetValue(SamplesProperty, value);
    }

    public IBrush? Stroke
    {
        get => GetValue(StrokeProperty);
        set => SetValue(StrokeProperty, value);
    }

    public double StrokeThickness
    {
        get => GetValue(StrokeThicknessProperty);
        set => SetValue(StrokeThicknessProperty, value);
    }

    public IBrush? Fill
    {
        get => GetValue(FillProperty);
        set => SetValue(FillProperty, value);
    }

    static SparklineControl()
    {
        AffectsRender<SparklineControl>(SamplesProperty, StrokeProperty, StrokeThicknessProperty, FillProperty);
    }

    public override void Render(DrawingContext context)
    {
        base.Render(context);

        var list = Samples?.ToList();
        if (list == null || list.Count < 2) return;

        double w = Bounds.Width;
        double h = Bounds.Height;
        if (w <= 0 || h <= 0) return;

        ulong max = Math.Max(list.Max(), 1024);
        int count = list.Count;

        var points = new Point[count];
        for (int i = 0; i < count; i++)
        {
            double x = (double)i / (count - 1) * w;
            double normalizedY = (double)list[i] / max;
            double y = h - (normalizedY * (h - 4)) - 2;
            points[i] = new Point(x, y);
        }

        // Draw area fill if Fill brush is provided
        if (Fill != null)
        {
            var areaGeometry = new StreamGeometry();
            using (var ctx = areaGeometry.Open())
            {
                ctx.BeginFigure(new Point(0, h), true);
                for (int i = 0; i < count; i++)
                {
                    ctx.LineTo(points[i]);
                }
                ctx.LineTo(new Point(w, h));
                ctx.EndFigure(true);
            }
            context.DrawGeometry(Fill, null, areaGeometry);
        }

        // Draw line
        var lineGeometry = new StreamGeometry();
        using (var ctx = lineGeometry.Open())
        {
            ctx.BeginFigure(points[0], false);
            for (int i = 1; i < count; i++)
            {
                ctx.LineTo(points[i]);
            }
            ctx.EndFigure(false);
        }

        var pen = new Pen(Stroke, StrokeThickness, lineCap: PenLineCap.Round, lineJoin: PenLineJoin.Round);
        context.DrawGeometry(null, pen, lineGeometry);
    }
}
