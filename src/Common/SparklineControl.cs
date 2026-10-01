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

    public static readonly StyledProperty<IEnumerable<ulong>?> SecondarySamplesProperty =
        AvaloniaProperty.Register<SparklineControl, IEnumerable<ulong>?>(nameof(SecondarySamples));

    public static readonly StyledProperty<IBrush?> SecondaryStrokeProperty =
        AvaloniaProperty.Register<SparklineControl, IBrush?>(nameof(SecondaryStroke), new SolidColorBrush(Color.Parse("#3b82f6")));

    public static readonly StyledProperty<IBrush?> SecondaryFillProperty =
        AvaloniaProperty.Register<SparklineControl, IBrush?>(nameof(SecondaryFill), new SolidColorBrush(Color.Parse("#203b82f6")));

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

    public IEnumerable<ulong>? SecondarySamples
    {
        get => GetValue(SecondarySamplesProperty);
        set => SetValue(SecondarySamplesProperty, value);
    }

    public IBrush? SecondaryStroke
    {
        get => GetValue(SecondaryStrokeProperty);
        set => SetValue(SecondaryStrokeProperty, value);
    }

    public IBrush? SecondaryFill
    {
        get => GetValue(SecondaryFillProperty);
        set => SetValue(SecondaryFillProperty, value);
    }

    static SparklineControl()
    {
        AffectsRender<SparklineControl>(SamplesProperty, StrokeProperty, StrokeThicknessProperty, FillProperty,
            SecondarySamplesProperty, SecondaryStrokeProperty, SecondaryFillProperty);
    }

    public override void Render(DrawingContext context)
    {
        base.Render(context);

        var list = Samples?.ToList();
        var secList = SecondarySamples?.ToList();
        if ((list == null || list.Count < 2) && (secList == null || secList.Count < 2)) return;

        double w = Bounds.Width;
        double h = Bounds.Height;
        if (w <= 0 || h <= 0) return;

        ulong max1 = list != null && list.Count > 0 ? list.Max() : 0;
        ulong max2 = secList != null && secList.Count > 0 ? secList.Max() : 0;
        ulong max = Math.Max(Math.Max(max1, max2), 1024);

        if (list != null && list.Count >= 2)
        {
            RenderSeries(context, list, max, w, h, Stroke, StrokeThickness, Fill);
        }

        if (secList != null && secList.Count >= 2)
        {
            RenderSeries(context, secList, max, w, h, SecondaryStroke, StrokeThickness, SecondaryFill);
        }
    }

    private static void RenderSeries(DrawingContext context, List<ulong> data, ulong max, double w, double h, IBrush? stroke, double thickness, IBrush? fill)
    {
        int count = data.Count;
        var points = new Point[count];
        for (int i = 0; i < count; i++)
        {
            double x = (double)i / (count - 1) * w;
            double normalizedY = (double)data[i] / max;
            double y = h - (normalizedY * (h - 4)) - 2;
            points[i] = new Point(x, y);
        }

        if (fill != null)
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
            context.DrawGeometry(fill, null, areaGeometry);
        }

        if (stroke != null)
        {
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
            var pen = new Pen(stroke, thickness, lineCap: PenLineCap.Round, lineJoin: PenLineJoin.Round);
            context.DrawGeometry(null, pen, lineGeometry);
        }
    }
}
