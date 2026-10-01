using System;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.Native;
using Prism.Services;

namespace Prism.ViewModels;

public partial class ClientOptimizerViewModel : ViewModelBase, INavigationAware
{
    private readonly NativeClientService _client = NativeClientService.Instance;
    private bool _isLoading;

    [ObservableProperty]
    private bool _optimizerEnabled = true;

    [ObservableProperty]
    private int _optimizerZstdLevel = 3;

    [ObservableProperty]
    private bool _optimizerAdaptiveFlush = true;

    [ObservableProperty]
    private int _optimizerFlushIntervalMs = 20;

    [ObservableProperty]
    private int _optimizerBufferThreshold = 65536;

    [ObservableProperty]
    private bool _savedNotice;

    public ClientOptimizerViewModel()
    {
        LoadConfig();
    }

    public void OnNavigatedTo()
    {
        LoadConfig();
    }

    public void OnNavigatedFrom()
    {
    }

    private void LoadConfig()
    {
        try
        {
            _isLoading = true;
            var cfg = _client.GetConfig();
            OptimizerEnabled = cfg.ActiveConfig.OptimizerEnabled;
            OptimizerZstdLevel = cfg.ActiveConfig.OptimizerZstdLevel;
            OptimizerAdaptiveFlush = cfg.ActiveConfig.OptimizerAdaptiveFlush;
            OptimizerFlushIntervalMs = (int)cfg.ActiveConfig.OptimizerFlushIntervalMs;
            OptimizerBufferThreshold = (int)cfg.ActiveConfig.OptimizerBufferThreshold;
        }
        catch
        {
        }
        finally
        {
            _isLoading = false;
        }
    }

    private void SaveConfig()
    {
        if (_isLoading) return;

        try
        {
            var cfg = _client.GetConfig();
            var patch = new ClientConfigPatch(
                ProfileName: null,
                ServerAddr: null,
                Transport: null,
                AuthToken: null,
                ListenAddr: null,
                FakeLanBroadcast: null,
                AutoConnectPanel: null,
                AutoConnect: null,
                ManagementUrl: null,
                TokenId: null,
                TokenType: null,
                UserId: null,
                Username: null,
                ExpiresAt: null,
                AutoCheckUpdate: null,
                UpdateChannel: null,
                Autostart: null,
                SilentAutostart: null,
                OptimizerEnabled: OptimizerEnabled,
                OptimizerZstdLevel: OptimizerZstdLevel,
                OptimizerAdaptiveFlush: OptimizerAdaptiveFlush,
                OptimizerFlushIntervalMs: (ulong)OptimizerFlushIntervalMs,
                OptimizerBufferThreshold: (ulong)OptimizerBufferThreshold
            );

            _client.SaveConfig(new SaveConfigRequest(
                ActiveProfileId: cfg.ActiveProfileId,
                ActiveConfig: patch
            ));

            SavedNotice = true;
            _ = Task.Delay(2000).ContinueWith(_ =>
            {
                Avalonia.Threading.Dispatcher.UIThread.Post(() => SavedNotice = false);
            });
        }
        catch
        {
        }
    }

    private System.Threading.CancellationTokenSource? _saveCts;

    private void DebouncedSaveConfig()
    {
        if (_isLoading) return;
        _saveCts?.Cancel();
        _saveCts = new System.Threading.CancellationTokenSource();
        var token = _saveCts.Token;

        Task.Delay(300, token).ContinueWith(t =>
        {
            if (!t.IsCanceled)
            {
                Avalonia.Threading.Dispatcher.UIThread.Post(() => SaveConfig());
            }
        }, TaskScheduler.Default);
    }

    [RelayCommand]
    public void ApplyPreset(string preset)
    {
        _isLoading = true;
        try
        {
            switch (preset.ToLowerInvariant())
            {
                case "gaming":
                    OptimizerEnabled = true;
                    OptimizerZstdLevel = 1;
                    OptimizerAdaptiveFlush = false;
                    OptimizerFlushIntervalMs = 5;
                    OptimizerBufferThreshold = 16384;
                    break;
                case "low_latency":
                    OptimizerEnabled = true;
                    OptimizerZstdLevel = 1;
                    OptimizerAdaptiveFlush = true;
                    OptimizerFlushIntervalMs = 10;
                    OptimizerBufferThreshold = 32768;
                    break;
                case "balanced":
                    OptimizerEnabled = true;
                    OptimizerZstdLevel = 3;
                    OptimizerAdaptiveFlush = true;
                    OptimizerFlushIntervalMs = 20;
                    OptimizerBufferThreshold = 65536;
                    break;
                case "max_compression":
                    OptimizerEnabled = true;
                    OptimizerZstdLevel = 9;
                    OptimizerAdaptiveFlush = true;
                    OptimizerFlushIntervalMs = 50;
                    OptimizerBufferThreshold = 131072;
                    break;
            }
        }
        finally
        {
            _isLoading = false;
        }
        SaveConfig();
    }

    [RelayCommand]
    public void GoToTraffic()
    {
        NavigationService.Instance.NavigateTo("client.traffic");
    }

    [RelayCommand]
    public void GoToMiddleware()
    {
        NavigationService.Instance.NavigateTo("client.middleware");
    }

    [RelayCommand]
    public void SetLevel(int level)
    {
        OptimizerZstdLevel = level;
        SaveConfig();
    }

    [RelayCommand]
    public void ResetDefaults()
    {
        ApplyPreset("balanced");
    }

    partial void OnOptimizerEnabledChanged(bool value) => DebouncedSaveConfig();
    partial void OnOptimizerZstdLevelChanged(int value) => DebouncedSaveConfig();
    partial void OnOptimizerAdaptiveFlushChanged(bool value) => DebouncedSaveConfig();
    partial void OnOptimizerFlushIntervalMsChanged(int value) => DebouncedSaveConfig();
    partial void OnOptimizerBufferThresholdChanged(int value) => DebouncedSaveConfig();
}
