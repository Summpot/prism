using System;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.Native;
using Prism.Services;

namespace Prism.ViewModels;

public partial class ClientOptimizerViewModel : ViewModelBase
{
    private readonly NativeClientService _client = NativeClientService.Instance;

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

    private void LoadConfig()
    {
        try
        {
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
    }

    private void SaveConfig()
    {
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
        }
        catch
        {
        }
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
        OptimizerEnabled = true;
        OptimizerZstdLevel = 3;
        OptimizerAdaptiveFlush = true;
        OptimizerFlushIntervalMs = 20;
        OptimizerBufferThreshold = 65536;
        SaveConfig();
    }

    partial void OnOptimizerEnabledChanged(bool value) => SaveConfig();
    partial void OnOptimizerZstdLevelChanged(int value) => SaveConfig();
    partial void OnOptimizerAdaptiveFlushChanged(bool value) => SaveConfig();
    partial void OnOptimizerFlushIntervalMsChanged(int value) => SaveConfig();
    partial void OnOptimizerBufferThresholdChanged(int value) => SaveConfig();
}
