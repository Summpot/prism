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

public partial class SettingsViewModel : ViewModelBase, INavigationAware
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
        new("en", "English")
    ];

    public static readonly string[] AvailableThemes = ["Default", "Light", "Dark"];
    public static readonly string[] AvailableUpdateChannels = ["stable", "beta", "alpha", "dev"];

    public string AppVersion
    {
        get
        {
            var version = typeof(App).Assembly.GetName().Version;
            return version == null ? "v0.1.0" : $"v{version.ToString(3)}";
        }
    }

    public string VersionChannelText => $"{AppVersion} ({UpdateChannel})";

    public string ArchitectureText =>
        $"{System.Runtime.InteropServices.RuntimeInformation.OSDescription} {System.Runtime.InteropServices.RuntimeInformation.ProcessArchitecture}";

    private bool _isLoading;

    public SettingsViewModel()
    {
        CurrentLocale = LocalizationManager.Instance.CurrentLocale;
        SelectedTheme = DesktopService.GetUiTheme();
        LoadConfig();
    }

    public void OnNavigatedTo()
    {
        CurrentLocale = LocalizationManager.Instance.CurrentLocale;
        SelectedTheme = DesktopService.GetUiTheme();
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
            AutoConnect = cfg.ActiveConfig.AutoConnect;
            AutoConnectPanel = cfg.ActiveConfig.AutoConnectPanel;
            FakeLanBroadcast = cfg.ActiveConfig.FakeLanBroadcast;
            DeviceId = !string.IsNullOrWhiteSpace(cfg.DeviceId) ? cfg.DeviceId : Guid.NewGuid().ToString("N")[..16];
            ManagementUrl = cfg.ActiveConfig.ManagementUrl ?? "";
            UpdateChannel = string.IsNullOrWhiteSpace(cfg.ActiveConfig.UpdateChannel) ? "stable" : cfg.ActiveConfig.UpdateChannel;
            AutoCheckUpdate = cfg.ActiveConfig.AutoCheckUpdate;
            Autostart = DesktopService.IsAutostartEnabled() || cfg.ActiveConfig.Autostart;
            SilentAutostart = cfg.ActiveConfig.SilentAutostart;
            IsAdmin = AdminApiClient.Instance.CurrentSession?.IsAdmin == true;
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
                FakeLanBroadcast: FakeLanBroadcast,
                AutoConnectPanel: AutoConnectPanel,
                AutoConnect: AutoConnect,
                ManagementUrl: string.IsNullOrWhiteSpace(ManagementUrl) ? null : ManagementUrl,
                TokenId: cfg.ActiveConfig.TokenId,
                TokenType: null,
                UserId: null,
                Username: null,
                ExpiresAt: null,
                AutoCheckUpdate: AutoCheckUpdate,
                UpdateChannel: UpdateChannel,
                Autostart: Autostart,
                SilentAutostart: SilentAutostart,
                OptimizerEnabled: null,
                OptimizerZstdLevel: null,
                OptimizerAdaptiveFlush: null,
                OptimizerFlushIntervalMs: null,
                OptimizerBufferThreshold: null
            );

            _client.SaveConfig(new SaveConfigRequest(
                ActiveProfileId: cfg.ActiveProfileId,
                ActiveConfig: patch
            ));
        }
        catch
        {
        }
    }

    partial void OnCurrentLocaleChanged(string value)
    {
        LocalizationManager.Instance.CurrentLocale = value;
        DesktopService.SetUiLocale(value);
    }

    partial void OnSelectedThemeChanged(string value)
    {
        DesktopService.SetUiTheme(value);
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

    partial void OnAutostartChanged(bool value)
    {
        if (_isLoading) return;
        DesktopService.SetAutostart(value, SilentAutostart);
        SaveConfig();
    }

    partial void OnSilentAutostartChanged(bool value)
    {
        if (_isLoading) return;
        if (Autostart)
        {
            DesktopService.SetAutostart(true, value);
        }
        SaveConfig();
    }

    partial void OnAutoConnectChanged(bool value) => SaveConfig();
    partial void OnAutoConnectPanelChanged(bool value) => SaveConfig();
    partial void OnFakeLanBroadcastChanged(bool value) => SaveConfig();
    partial void OnUpdateChannelChanged(string value)
    {
        SaveConfig();
        OnPropertyChanged(nameof(VersionChannelText));
    }
    partial void OnAutoCheckUpdateChanged(bool value) => SaveConfig();
    partial void OnManagementUrlChanged(string value) => SaveConfig();

    [ObservableProperty]
    private bool _isAdmin;

    [ObservableProperty]
    private bool _copiedDeviceIdNotice;

    [RelayCommand]
    public void SelectTab(string tab)
    {
        ActiveTab = tab;
    }

    [RelayCommand]
    public void SelectTheme(string theme)
    {
        SelectedTheme = theme;
    }

    [RelayCommand]
    public void SelectLocale(string locale)
    {
        CurrentLocale = locale;
    }

    [RelayCommand]
    public void GoToProfiles()
    {
        NavigationService.Instance.NavigateTo("client.profiles");
    }

    [RelayCommand]
    public void GoToAdmin()
    {
        NavigationService.Instance.NavigateTo("admin.overview");
    }

    [RelayCommand]
    public void OpenGitHub()
    {
        _client.OpenExternalUrl("https://github.com/Summpot/prism");
    }

    [RelayCommand]
    public async Task CopyDeviceIdAsync()
    {
        if (Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop &&
            desktop.MainWindow?.Clipboard != null)
        {
            await desktop.MainWindow.Clipboard.SetTextAsync(DeviceId);
            CopiedDeviceIdNotice = true;
            _ = Task.Delay(2000).ContinueWith(_ =>
            {
                Avalonia.Threading.Dispatcher.UIThread.Post(() => CopiedDeviceIdNotice = false);
            });
        }
    }

    [ObservableProperty]
    private bool _isInstallingUpdate;

    [RelayCommand]
    public async Task CheckUpdateAsync()
    {
        try
        {
            IsCheckingUpdate = true;
            UpdateStatusMessage = LocalizationManager.Instance["client_update_checking"] ?? "Checking for updates...";
            var res = await _client.CheckUpdateAsync(UpdateChannel);
            UpdateAvailable = res.Available;
            LatestVersion = res.Version ?? "";
            UpdateChangelog = res.Body ?? "";
            if (res.Available)
            {
                UpdateStatusMessage = I18nText.Format("client_update_found", ("version", res.Version ?? ""));
            }
            else
            {
                UpdateStatusMessage = LocalizationManager.Instance["client_update_up_to_date"] ?? "You are already using the latest version.";
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

    [RelayCommand]
    public async Task InstallUpdateAsync()
    {
        try
        {
            IsInstallingUpdate = true;
            UpdateStatusMessage = LocalizationManager.Instance["client_update_installing"] ?? "Downloading and applying update...";
            await _client.InstallUpdateAsync(UpdateChannel);
            UpdateStatusMessage = I18nText.T("client_update_prompt_updating");
        }
        catch (Exception ex)
        {
            UpdateStatusMessage = I18nText.Format("client_update_failed", ("error", ex.Message));
            _client.OpenExternalUrl("https://github.com/Summpot/prism/releases");
            AppServices.ShowInfo(I18nText.T("client_update_open_releases"));
        }
        finally
        {
            IsInstallingUpdate = false;
        }
    }
}
