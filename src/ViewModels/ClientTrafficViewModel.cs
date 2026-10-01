using System;
using System.Collections.ObjectModel;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using LiveChartsCore;
using LiveChartsCore.SkiaSharpView;
using LiveChartsCore.SkiaSharpView.Painting;
using Prism.Common;
using Prism.I18n;
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
    private bool _isConnected;

    [ObservableProperty]
    private bool _hasTrafficData;

    [ObservableProperty]
    private bool _showEmptyState = true;

    [ObservableProperty]
    private string _metric1Title = "运行时间";

    [ObservableProperty]
    private string _metric1Value = "0s";

    [ObservableProperty]
    private string _rawBytesText = "0 B";

    [ObservableProperty]
    private string _wireBytesText = "0 B";

    [ObservableProperty]
    private string _savedRatioText = "0.0%";

    [ObservableProperty]
    private string _currentThroughputText = "0 B/s";

    [ObservableProperty]
    private string _sectionTitle = "当前会话";

    [ObservableProperty]
    private string? _actualTransport;

    [ObservableProperty]
    private bool _hasTransport;

    [ObservableProperty]
    private bool _isSessionMode = true;

    [ObservableProperty]
    private bool _isLifetimeMode;

    // Session stats
    [ObservableProperty]
    private string _sessionRawBytes = "0 B";

    [ObservableProperty]
    private string _sessionWireBytes = "0 B";

    [ObservableProperty]
    private string _sessionSavedText = "0 B (0.0%)";

    [ObservableProperty]
    private string _sessionLinkRate = "0 bps (估计)";

    [ObservableProperty]
    private string _urgentBatchesText = "0";

    [ObservableProperty]
    private string _timerBatchesText = "0";

    [ObservableProperty]
    private string _thresholdBatchesText = "0";

    // Uplink
    [ObservableProperty]
    private string _uplinkRawWireText = "0 B → 0 B";

    [ObservableProperty]
    private string _uplinkSavedRatio = "0.0%";

    [ObservableProperty]
    private string _uplinkBatches = "0";

    [ObservableProperty]
    private string _uplinkBatchingP50 = "0µs";

    [ObservableProperty]
    private string _uplinkBatchingP90 = "0µs";

    [ObservableProperty]
    private string _uplinkBatchingP99 = "0µs";

    [ObservableProperty]
    private string _uplinkBatchingMax = "0µs";

    [ObservableProperty]
    private string _uplinkCompP50 = "0µs";

    [ObservableProperty]
    private string _uplinkCompP90 = "0µs";

    [ObservableProperty]
    private string _uplinkCompP99 = "0µs";

    [ObservableProperty]
    private string _uplinkCompMax = "0µs";

    // Downlink
    [ObservableProperty]
    private string _downlinkRawWireText = "0 B → 0 B";

    [ObservableProperty]
    private string _downlinkSavedRatio = "0.0%";

    [ObservableProperty]
    private string _downlinkBatches = "0";

    [ObservableProperty]
    private string _downlinkBatchingP50 = "0µs";

    [ObservableProperty]
    private string _downlinkBatchingP90 = "0µs";

    [ObservableProperty]
    private string _downlinkBatchingP99 = "0µs";

    [ObservableProperty]
    private string _downlinkBatchingMax = "0µs";

    [ObservableProperty]
    private string _downlinkCompP50 = "0µs";

    [ObservableProperty]
    private string _downlinkCompP90 = "0µs";

    [ObservableProperty]
    private string _downlinkCompP99 = "0µs";

    [ObservableProperty]
    private string _downlinkCompMax = "0µs";

    // Lifetime stats
    [ObservableProperty]
    private string _lifetimeRawText = "0 B";

    [ObservableProperty]
    private string _lifetimeWireText = "0 B";

    [ObservableProperty]
    private string _lifetimeSavedText = "0 B (0.0%)";

    public ObservableCollection<double> ChartValues { get; } = new();
    public ISeries[] Series { get; }

    public ClientTrafficViewModel()
    {
        Series = new ISeries[]
        {
            new LineSeries<double>
            {
                Values = ChartValues,
                Fill = new SolidColorPaint(new SKColor(16, 185, 129, 25)),
                Stroke = new SolidColorPaint(new SKColor(16, 185, 129)) { StrokeThickness = 1.5f },
                GeometrySize = 0,
                LineSmoothness = 0.65
            }
        };

        _client.StatusUpdated += OnStatusUpdated;
        _client.ThroughputSampleAdded += OnThroughputSample;

        // Initialize state
        if (_client.CurrentStatus != null)
        {
            OnStatusUpdated(_client.CurrentStatus);
        }
        else
        {
            UpdateProperties(null);
        }
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
        if (!_isNavigatedTo && ChartValues.Count > 0) return;
        UpdateProperties(status);
    }

    private void UpdateProperties(ClientStatusResponse? status)
    {
        IsConnected = status?.Running == true;
        var cum = status?.CumulativeStats;
        ulong cumRaw = cum?.RawBytes ?? 0;
        ulong cumWire = cum?.WireBytes ?? 0;
        ulong cumSessions = cum?.SessionsCount ?? 0;

        HasTrafficData = IsConnected || cumRaw > 0;
        ShowEmptyState = !HasTrafficData;

        ActualTransport = status?.ActualTransport ?? status?.Transport;
        HasTransport = IsConnected && !string.IsNullOrEmpty(ActualTransport);

        ulong currentRaw = status?.Stats.RawBytes ?? 0;
        ulong currentWire = status?.Stats.WireBytes ?? 0;
        double currentSavedRatio = status?.Stats.SavedRatio ?? 0;

        ulong lifetimeRaw = cumRaw + (IsConnected ? currentRaw : 0);
        ulong lifetimeWire = cumWire + (IsConnected ? currentWire : 0);
        double lifetimeSavedRatio = lifetimeRaw > 0 && lifetimeWire <= lifetimeRaw
            ? ((double)(lifetimeRaw - lifetimeWire) / lifetimeRaw) * 100.0
            : 0;

        IsSessionMode = StatsViewMode == "session";
        IsLifetimeMode = StatsViewMode == "lifetime";

        if (IsSessionMode)
        {
            Metric1Title = LocalizationManager.Instance["client_uptime"] ?? "运行时间";
            Metric1Value = Formatters.FormatUptime(_client.UptimeSeconds);
            RawBytesText = Formatters.FormatBytes(currentRaw);
            WireBytesText = Formatters.FormatBytes(currentWire);
            SavedRatioText = Formatters.FormatPercentage(currentSavedRatio);
            SectionTitle = LocalizationManager.Instance["client_current_session"] ?? "当前会话";
        }
        else
        {
            Metric1Title = LocalizationManager.Instance["client_sessions"] ?? "会话数";
            Metric1Value = $"{cumSessions + (IsConnected ? 1UL : 0UL)}";
            RawBytesText = Formatters.FormatBytes(lifetimeRaw);
            WireBytesText = Formatters.FormatBytes(lifetimeWire);
            SavedRatioText = $"{lifetimeSavedRatio:0.#}%";
            SectionTitle = LocalizationManager.Instance["client_cumulative_lifetime"] ?? "累计生命周期";
        }

        if (status != null)
        {
            var opt = status.Stats;
            SessionRawBytes = Formatters.FormatBytes(opt.RawBytes);
            SessionWireBytes = Formatters.FormatBytes(opt.WireBytes);
            SessionSavedText = $"{Formatters.FormatBytes(opt.SavedBytes)} ({Formatters.FormatPercentage(opt.SavedRatio)})";

            string rateMode = opt.LinkRateMeasured
                ? (LocalizationManager.Instance["traffic_link_measured"] ?? "实测")
                : (LocalizationManager.Instance["traffic_link_estimated"] ?? "估计");
            SessionLinkRate = $"{Formatters.FormatBitsPerSecond(opt.LinkRateBps)} ({rateMode})";

            UrgentBatchesText = $"{opt.UrgentBatches}";
            TimerBatchesText = $"{opt.TimerBatches}";
            ThresholdBatchesText = $"{opt.ThresholdBatches}";

            var up = opt.Uplink;
            UplinkRawWireText = $"{Formatters.FormatBytes(up.RawBytes)} → {Formatters.FormatBytes(up.WireBytes)}";
            UplinkSavedRatio = Formatters.FormatPercentage(up.SavedRatio);
            UplinkBatches = $"{up.Batches}";
            UplinkBatchingP50 = $"{up.BatchingDelay.P50Us:0}µs";
            UplinkBatchingP90 = $"{up.BatchingDelay.P90Us:0}µs";
            UplinkBatchingP99 = $"{up.BatchingDelay.P99Us:0}µs";
            UplinkBatchingMax = $"{up.BatchingDelay.MaxUs:0}µs";

            UplinkCompP50 = $"{up.CompressionTime.P50Us:0}µs";
            UplinkCompP90 = $"{up.CompressionTime.P90Us:0}µs";
            UplinkCompP99 = $"{up.CompressionTime.P99Us:0}µs";
            UplinkCompMax = $"{up.CompressionTime.MaxUs:0}µs";

            var down = opt.Downlink;
            DownlinkRawWireText = $"{Formatters.FormatBytes(down.RawBytes)} → {Formatters.FormatBytes(down.WireBytes)}";
            DownlinkSavedRatio = Formatters.FormatPercentage(down.SavedRatio);
            DownlinkBatches = $"{down.Batches}";
            DownlinkBatchingP50 = $"{down.BatchingDelay.P50Us:0}µs";
            DownlinkBatchingP90 = $"{down.BatchingDelay.P90Us:0}µs";
            DownlinkBatchingP99 = $"{down.BatchingDelay.P99Us:0}µs";
            DownlinkBatchingMax = $"{down.BatchingDelay.MaxUs:0}µs";

            DownlinkCompP50 = $"{down.CompressionTime.P50Us:0}µs";
            DownlinkCompP90 = $"{down.CompressionTime.P90Us:0}µs";
            DownlinkCompP99 = $"{down.CompressionTime.P99Us:0}µs";
            DownlinkCompMax = $"{down.CompressionTime.MaxUs:0}µs";
        }

        LifetimeRawText = Formatters.FormatBytes(lifetimeRaw);
        LifetimeWireText = Formatters.FormatBytes(lifetimeWire);
        ulong savedBytes = lifetimeRaw >= lifetimeWire ? lifetimeRaw - lifetimeWire : 0;
        LifetimeSavedText = $"{Formatters.FormatBytes(savedBytes)} ({lifetimeSavedRatio:0.#}%)";
    }

    [RelayCommand]
    public void SetStatsMode(string mode)
    {
        StatsViewMode = mode;
        UpdateProperties(_client.CurrentStatus);
    }

    [RelayCommand]
    public void ResetStats()
    {
        _client.ResetStats();
        UpdateProperties(_client.CurrentStatus);
    }
}
