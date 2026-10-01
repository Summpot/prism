using System;
using System.Collections.Generic;
using System.Threading;
using System.Threading.Tasks;
using Avalonia.Threading;
using Prism.Native;

namespace Prism.Services;

public class NativeClientService
{
    private static readonly Lazy<NativeClientService> _instance = new(() => new NativeClientService());
    public static NativeClientService Instance => _instance.Value;

    private readonly DispatcherTimer _pollTimer;
    private ulong _lastWireBytes = 0;
    private DateTime _lastThroughputCheck = DateTime.UtcNow;

    public ClientStatusResponse? CurrentStatus { get; private set; }
    public List<ClientLogEntry> CurrentLogs { get; private set; } = new();
    public List<ulong> ThroughputHistory { get; } = new();

    public event Action<ClientStatusResponse>? StatusUpdated;
    public event Action<List<ClientLogEntry>>? LogsUpdated;
    public event Action<ulong>? ThroughputSampleAdded;

    public NativeClientService()
    {
        try
        {
            PrismNativeMethods.InitClient(null);
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[WARN] InitClient notice: {ex.Message}");
        }

        _pollTimer = new DispatcherTimer
        {
            Interval = TimeSpan.FromSeconds(1)
        };
        _pollTimer.Tick += OnPollTick;
        _pollTimer.Start();
    }

    private DateTime? _startedAt;
    public ulong UptimeSeconds => _startedAt.HasValue && CurrentStatus?.Running == true ? (ulong)(DateTime.UtcNow - _startedAt.Value).TotalSeconds : 0;

    private async void OnPollTick(object? sender, EventArgs e)
    {
        try
        {
            var status = await PrismNativeMethods.ClientStatusAsync();
            CurrentStatus = status;

            if (status.Running)
            {
                if (!_startedAt.HasValue)
                {
                    _startedAt = DateTime.UtcNow;
                }
            }
            else
            {
                _startedAt = null;
            }

            StatusUpdated?.Invoke(status);

            // Calculate throughput if running
            if (status.Running)
            {
                var now = DateTime.UtcNow;
                var elapsedSec = (now - _lastThroughputCheck).TotalSeconds;
                if (elapsedSec > 0.1)
                {
                    ulong currentWire = status.Stats.WireBytes;
                    ulong diff = currentWire >= _lastWireBytes && _lastWireBytes > 0 ? currentWire - _lastWireBytes : 0;
                    _lastWireBytes = currentWire;
                    _lastThroughputCheck = now;

                    ulong bytesPerSec = (ulong)(diff / elapsedSec);
                    ThroughputHistory.Add(bytesPerSec);
                    if (ThroughputHistory.Count > 30)
                    {
                        ThroughputHistory.RemoveAt(0);
                    }
                    ThroughputSampleAdded?.Invoke(bytesPerSec);
                }
            }
            else
            {
                _lastWireBytes = 0;
            }

            if (LogsUpdated != null && (NavigationService.Instance.CurrentRoute == "client.logs" || CurrentLogs.Count == 0))
            {
                var logs = await PrismNativeMethods.ClientLogsAsync(200);
                CurrentLogs = logs;
                LogsUpdated?.Invoke(logs);
            }
        }
        catch
        {
            // Suppress background poll errors
        }
    }

    public async Task RefreshLogsAsync()
    {
        try
        {
            var logs = await PrismNativeMethods.ClientLogsAsync(200);
            CurrentLogs = logs;
            LogsUpdated?.Invoke(logs);
        }
        catch { }
    }

    public async Task<ClientStatusResponse> GetStatusAsync()
    {
        var s = await PrismNativeMethods.ClientStatusAsync();
        CurrentStatus = s;
        return s;
    }

    public async Task StartAsync(string? profileId = null, string? serverAddr = null, string? transport = null, string? authToken = null)
    {
        var cfg = GetConfig();
        string activeServer = serverAddr ?? cfg.ActiveConfig.ServerAddr;
        string activeTransport = transport ?? cfg.ActiveConfig.Transport;
        string activeAuth = authToken ?? cfg.ActiveConfig.AuthToken;
        string activeListen = cfg.ActiveConfig.ListenAddr;
        bool activeFakeLan = cfg.ActiveConfig.FakeLanBroadcast;

        var opt = new ClientOptimizerConfig(
            Enabled: cfg.ActiveConfig.OptimizerEnabled,
            ZstdLevel: cfg.ActiveConfig.OptimizerZstdLevel,
            AdaptiveFlush: cfg.ActiveConfig.OptimizerAdaptiveFlush,
            FlushIntervalMs: cfg.ActiveConfig.OptimizerFlushIntervalMs,
            BufferThreshold: cfg.ActiveConfig.OptimizerBufferThreshold
        );

        var req = new StartClientRequest(
            ServerAddr: activeServer,
            Transport: activeTransport,
            AuthToken: activeAuth,
            ListenAddr: activeListen,
            Middleware: null,
            FakeLanBroadcast: activeFakeLan,
            MotdPrefix: "[Prism]",
            Optimizer: opt,
            ProfileId: profileId,
            ProfileName: cfg.ActiveConfig.ProfileName
        );

        await PrismNativeMethods.ClientStartAsync(req);
        await GetStatusAsync();
    }

    public async Task StopAsync()
    {
        await PrismNativeMethods.ClientStopAsync();
        await GetStatusAsync();
    }

    public void ResetStats()
    {
        PrismNativeMethods.ClientResetStats();
    }

    public List<ClientProfile> GetProfiles()
    {
        return PrismNativeMethods.ClientGetProfiles();
    }

    public void SaveProfiles(List<ClientProfile> profiles)
    {
        PrismNativeMethods.ClientSaveProfiles(profiles);
    }

    public ClientConfigResponse GetConfig()
    {
        return PrismNativeMethods.ClientGetConfig();
    }

    public void SaveConfig(SaveConfigRequest req)
    {
        PrismNativeMethods.ClientSaveConfig(req);
    }

    public async Task<List<ClientLogEntry>> GetLogsAsync(uint? limit = 200)
    {
        return await PrismNativeMethods.ClientLogsAsync(limit);
    }

    public async Task ClearLogsAsync()
    {
        await PrismNativeMethods.ClientClearLogsAsync();
        CurrentLogs.Clear();
        LogsUpdated?.Invoke(CurrentLogs);
    }

    public List<MiddlewareItem> ListMiddlewares()
    {
        return PrismNativeMethods.ClientListMiddlewares();
    }

    public MiddlewareItem UpdateMiddlewareConfig(string name, Dictionary<string, string> config)
    {
        return PrismNativeMethods.ClientUpdateMiddlewareConfig(name, config);
    }

    public MiddlewareItem ResetMiddlewareConfig(string name)
    {
        return PrismNativeMethods.ClientResetMiddlewareConfig(name);
    }

    public async Task<UpdateCheckResponse> CheckUpdateAsync(string? channel)
    {
        return await PrismNativeMethods.ClientCheckUpdateAsync(channel);
    }

    public async Task InstallUpdateAsync(string? channel)
    {
        await PrismNativeMethods.ClientInstallUpdateAsync(channel);
    }

    public async Task<AdminHttpResponse> AdminRequestAsync(AdminHttpRequest payload)
    {
        return await PrismNativeMethods.AdminRequestAsync(payload);
    }

    public async Task<AdminRpcResponse> ControlRpcAsync(AdminRpcRequest payload)
    {
        return await PrismNativeMethods.ControlRpcAsync(payload);
    }

    public void OpenExternalUrl(string url)
    {
        PrismNativeMethods.OpenExternalUrl(url);
    }
}
