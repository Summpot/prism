using System;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Input.Platform;
using Avalonia.Styling;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.I18n;
using Prism.Native;
using Prism.Services;

namespace Prism.ViewModels;

public partial class SettingsViewModel : ViewModelBase
{
    private readonly NativeClientService _client = NativeClientService.Instance;

    [ObservableProperty]
    private string _activeTab = "general";

    [ObservableProperty]
    private string _currentLocale = "zh-CN";

    [ObservableProperty]
    private string _selectedTheme = "Default";

    [ObservableProperty]
    private bool _autoConnect;

    [ObservableProperty]
    private bool _autostart;

    [ObservableProperty]
    private bool _silentAutostart;

    [ObservableProperty]
    private bool _autoConnectPanel;

    [ObservableProperty]
    private bool _fakeLanBroadcast;

    [ObservableProperty]
    private string _deviceId = "";

    [ObservableProperty]
    private string _managementUrl = "";

    [ObservableProperty]
    private string _updateChannel = "stable";

    [ObservableProperty]
    private bool _autoCheckUpdate = true;

    [ObservableProperty]
    private string? _updateStatusMessage;

    [ObservableProperty]
    private bool _isCheckingUpdate;

    [ObservableProperty]
    private bool _updateAvailable;

    [ObservableProperty]
    private string? _latestVersion;

    [ObservableProperty]
    private string? _updateChangelog;

    public static readonly Common.LocaleOption[] AvailableLanguages =
    [
        new("zh-CN", "简体中文"),
        new("en-US", "English"),
        new("ja-JP", "日本語"),
        new("zh-TW", "繁體中文")
    ];

    public static readonly string[] AvailableThemes = ["Default", "Light", "Dark"];
    public static readonly string[] AvailableUpdateChannels = ["stable", "beta"];

    public SettingsViewModel()
    {
        CurrentLocale = LocalizationManager.Instance.CurrentLocale;
        LoadConfig();
    }

    private void LoadConfig()
    {
        try
        {
            var cfg = _client.GetConfig();
            AutoConnect = cfg.ActiveConfig.AutoConnect;
            AutoConnectPanel = cfg.ActiveConfig.AutoConnectPanel;
            FakeLanBroadcast = cfg.ActiveConfig.FakeLanBroadcast;
            DeviceId = !string.IsNullOrWhiteSpace(cfg.ActiveConfig.TokenId) ? cfg.ActiveConfig.TokenId : Guid.NewGuid().ToString("N")[..16];
            ManagementUrl = cfg.ActiveConfig.ManagementUrl ?? "";
        }
        catch
        {
        }
    }

    partial void OnCurrentLocaleChanged(string value)
    {
        LocalizationManager.Instance.CurrentLocale = value;
    }

    partial void OnSelectedThemeChanged(string value)
    {
        if (Application.Current != null)
        {
            Application.Current.RequestedThemeVariant = value switch
            {
                "Light" => ThemeVariant.Light,
                "Dark" => ThemeVariant.Dark,
                _ => ThemeVariant.Default
            };
        }
    }

    [RelayCommand]
    public async Task CopyDeviceIdAsync()
    {
        if (Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop &&
            desktop.MainWindow?.Clipboard != null)
        {
            await desktop.MainWindow.Clipboard.SetTextAsync(DeviceId);
        }
    }

    [RelayCommand]
    public async Task CheckUpdateAsync()
    {
        try
        {
            IsCheckingUpdate = true;
            UpdateStatusMessage = "Checking for updates...";
            var res = await _client.CheckUpdateAsync(UpdateChannel);
            if (res.Available)
            {
                UpdateStatusMessage = $"Update available: {res.Version}";
            }
            else
            {
                UpdateStatusMessage = "You are already using the latest version.";
            }
        }
        catch (Exception ex)
        {
            UpdateStatusMessage = $"Check failed: {ex.Message}";
        }
        finally
        {
            IsCheckingUpdate = false;
        }
    }
}
