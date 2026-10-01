using System;
using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using LiveChartsCore;
using LiveChartsCore.SkiaSharpView;
using LiveChartsCore.SkiaSharpView.Painting;
using Prism.Common;
using Prism.Native;
using Prism.Services;
using SkiaSharp;

namespace Prism.ViewModels;

public partial class ClientTrafficViewModel : ViewModelBase, INavigationAware
{
    private readonly NativeClientService _client = NativeClientService.Instance;
    private bool _isNavigatedTo;

    [ObservableProperty]
    private string _statsViewMode = "session";

    [ObservableProperty]
    private string _uptimeText = "0s";

    [ObservableProperty]
    private string _rawBytesText = "0 B";

    [ObservableProperty]
    private string _wireBytesText = "0 B";

    [ObservableProperty]
    private string _savedRatioText = "0.0%";

    [ObservableProperty]
    private string _currentThroughputText = "0 B/s";

    [ObservableProperty]
    private string _linkRateText = "0 bps (Estimated)";

    [ObservableProperty]
    private string _urgentBatchesText = "0";

    [ObservableProperty]
    private string _timerBatchesText = "0";

    [ObservableProperty]
    private string _thresholdBatchesText = "0";

    [ObservableProperty]
    private string _uplinkRawWireText = "0 B → 0 B";

    [ObservableProperty]
    private string _downlinkRawWireText = "0 B → 0 B";

    [ObservableProperty]
    private string _uplinkP50 = "0µs";

    [ObservableProperty]
    private string _uplinkP90 = "0µs";

    [ObservableProperty]
    private string _uplinkP99 = "0µs";

    [ObservableProperty]
    private string _uplinkMax = "0µs";

    [ObservableProperty]
    private string _downlinkP50 = "0µs";

    [ObservableProperty]
    private string _downlinkP90 = "0µs";

    [ObservableProperty]
    private string _downlinkP99 = "0µs";

    [ObservableProperty]
    private string _downlinkMax = "0µs";

    public ObservableCollection<double> ChartValues { get; } = new();
    public ISeries[] Series { get; }

    public ClientTrafficViewModel()
    {
        Series = new ISeries[]
        {
            new LineSeries<double>
            {
                Values = ChartValues,
                Fill = new SolidColorPaint(new SKColor(16, 185, 129, 30)),
                Stroke = new SolidColorPaint(new SKColor(16, 185, 129)) { StrokeThickness = 2 },
                GeometrySize = 0,
                LineSmoothness = 0.65
            }
        };

        _client.StatusUpdated += OnStatusUpdated;
        _client.ThroughputSampleAdded += OnThroughputSample;
    }

    public void OnNavigatedTo()
    {
        _isNavigatedTo = true;
        if (_client.CurrentStatus != null)
        {
            OnStatusUpdated(_client.CurrentStatus);
        }
    }

    public void OnNavigatedFrom()
    {
        _isNavigatedTo = false;
    }

    private void OnThroughputSample(ulong bps)
    {
        if (!_isNavigatedTo) return;
        CurrentThroughputText = $"{Formatters.FormatBytes(bps)}/s";
        ChartValues.Add((double)bps);
        if (ChartValues.Count > 30)
        {
            ChartValues.RemoveAt(0);
        }
    }

    private void OnStatusUpdated(ClientStatusResponse status)
    {
        if (!_isNavigatedTo) return;
        if (StatsViewMode == "session")
        {
            UptimeText = Formatters.FormatUptime(_client.UptimeSeconds);
            RawBytesText = Formatters.FormatBytes(status.Stats.RawBytes);
            WireBytesText = Formatters.FormatBytes(status.Stats.WireBytes);
            SavedRatioText = Formatters.FormatPercentage(status.Stats.SavedRatio);
        }
        else
        {
            var cum = status.CumulativeStats;
            if (cum != null)
            {
                UptimeText = $"{cum.SessionsCount + 1}";
                RawBytesText = Formatters.FormatBytes(cum.RawBytes + status.Stats.RawBytes);
                WireBytesText = Formatters.FormatBytes(cum.WireBytes + status.Stats.WireBytes);
                SavedRatioText = Formatters.FormatPercentage(cum.SavedRatio);
            }
            else
            {
                UptimeText = Formatters.FormatUptime(_client.UptimeSeconds);
                RawBytesText = Formatters.FormatBytes(status.Stats.RawBytes);
                WireBytesText = Formatters.FormatBytes(status.Stats.WireBytes);
                SavedRatioText = Formatters.FormatPercentage(status.Stats.SavedRatio);
            }
        }

        var opt = status.Stats;
        UrgentBatchesText = $"{opt.UrgentBatches}";
        TimerBatchesText = $"{opt.TimerBatches}";
        ThresholdBatchesText = $"{opt.ThresholdBatches}";

        var up = opt.Uplink;
        var down = opt.Downlink;

        UplinkRawWireText = $"{Formatters.FormatBytes(up.RawBytes)} → {Formatters.FormatBytes(up.WireBytes)} ({Formatters.FormatPercentage(up.SavedRatio)})";
        DownlinkRawWireText = $"{Formatters.FormatBytes(down.RawBytes)} → {Formatters.FormatBytes(down.WireBytes)} ({Formatters.FormatPercentage(down.SavedRatio)})";

        UplinkP50 = $"{up.BatchingDelay.P50Us}µs";
        UplinkP90 = $"{up.BatchingDelay.P90Us}µs";
        UplinkP99 = $"{up.BatchingDelay.P99Us}µs";
        UplinkMax = $"{up.BatchingDelay.MaxUs}µs";

        DownlinkP50 = $"{down.CompressionTime.P50Us}µs";
        DownlinkP90 = $"{down.CompressionTime.P90Us}µs";
        DownlinkP99 = $"{down.CompressionTime.P99Us}µs";
        DownlinkMax = $"{down.CompressionTime.MaxUs}µs";
    }

    [RelayCommand]
    public void SetStatsMode(string mode)
    {
        StatsViewMode = mode;
        if (_client.CurrentStatus != null)
        {
            OnStatusUpdated(_client.CurrentStatus);
        }
    }

    [RelayCommand]
    public void ResetStats()
    {
        _client.ResetStats();
    }
}
