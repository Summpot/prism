using System;
using Avalonia;
using Avalonia.Controls;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.I18n;
using Prism.Services;
using ShadUI;
using Window = Avalonia.Controls.Window;

namespace Prism.ViewModels;

public partial class MainWindowViewModel : ViewModelBase
{
    public NavigationService Navigation { get; } = NavigationService.Instance;
    public NativeClientService NativeClient { get; } = NativeClientService.Instance;

    public DialogManager DialogManager { get; } = new();
    public ToastManager ToastManager { get; } = new();

    [ObservableProperty]
    private bool _isSidebarExpanded = true;

    [ObservableProperty]
    private string _currentLocale = "zh-CN";

    public static readonly Common.LocaleOption[] AvailableLocales =
    [
        new("zh-CN", "简体中文"),
        new("en", "English")
    ];

    partial void OnCurrentLocaleChanged(string value)
    {
        LocalizationManager.Instance.CurrentLocale = value;
    }

    [ObservableProperty]
    private bool _isAdmin = true;

    [ObservableProperty]
    private ViewModelBase _currentPageViewModel;

    // ViewModels cache
    private readonly ClientOverviewViewModel _clientOverview;
    private readonly ClientTrafficViewModel _clientTraffic;
    private readonly ClientLogsViewModel _clientLogs;
    private readonly ClientProfilesViewModel _clientProfiles;
    private readonly ClientMiddlewareViewModel _clientMiddleware;
    private readonly ClientOptimizerViewModel _clientOptimizer;
    private readonly SettingsViewModel _settings;
    private readonly AdminOverviewViewModel _adminOverview;
    private readonly AdminConnectionsViewModel _adminConnections;
    private readonly AdminTunnelServicesViewModel _adminServices;
    private readonly AdminTrafficViewModel _adminTraffic;
    private readonly AdminConnectorsViewModel _adminConnectors;
    private readonly AdminMiddlewareViewModel _adminMiddleware;
    private readonly AdminRuntimeViewModel _adminRuntime;
    private readonly AdminUsersViewModel _adminUsers;

    public MainWindowViewModel()
    {
        CurrentLocale = LocalizationManager.Instance.CurrentLocale;

        _clientOverview = new ClientOverviewViewModel();
        _clientTraffic = new ClientTrafficViewModel();
        _clientLogs = new ClientLogsViewModel();
        _clientProfiles = new ClientProfilesViewModel();
        _clientMiddleware = new ClientMiddlewareViewModel();
        _clientOptimizer = new ClientOptimizerViewModel();
        _settings = new SettingsViewModel();
        _adminOverview = new AdminOverviewViewModel();
        _adminConnections = new AdminConnectionsViewModel();
        _adminServices = new AdminTunnelServicesViewModel();
        _adminTraffic = new AdminTrafficViewModel();
        _adminConnectors = new AdminConnectorsViewModel();
        _adminMiddleware = new AdminMiddlewareViewModel();
        _adminRuntime = new AdminRuntimeViewModel();
        _adminUsers = new AdminUsersViewModel();

        _currentPageViewModel = _clientOverview;

        Navigation.Navigated += OnNavigated;

        AdminApiClient.Instance.SessionUpdated += s =>
        {
            Avalonia.Threading.Dispatcher.UIThread.Post(() =>
            {
                if (s != null)
                {
                    IsAdmin = s.IsAdmin;
                }
            });
        };

        NativeClient.StatusUpdated += status =>
        {
            Avalonia.Threading.Dispatcher.UIThread.Post(() =>
            {
                if (!status.Running)
                {
                    IsAdmin = true;
                }
            });
        };
    }

    private void OnNavigated(string route)
    {
        (CurrentPageViewModel as INavigationAware)?.OnNavigatedFrom();

        var newVm = route switch
        {
            "client.overview" => (ViewModelBase)_clientOverview,
            "client.traffic" => _clientTraffic,
            "client.logs" => _clientLogs,
            "client.profiles" => _clientProfiles,
            "client.middleware" or "client.middlewares" => _clientMiddleware,
            "client.optimizer" => _clientOptimizer,
            "settings" or "client.settings" => _settings,
            "admin.overview" => _adminOverview,
            "admin.connections" => _adminConnections,
            "admin.services" => _adminServices,
            "admin.traffic" => _adminTraffic,
            "admin.connectors" => _adminConnectors,
            "admin.middleware" => _adminMiddleware,
            "admin.runtime" => _adminRuntime,
            "admin.users" => _adminUsers,
            _ => _clientOverview
        };

        CurrentPageViewModel = newVm;
        (newVm as INavigationAware)?.OnNavigatedTo();
    }

    [RelayCommand]
    public void Navigate(string route)
    {
        Navigation.NavigateTo(route);
    }

    [RelayCommand]
    public void ToggleSidebar()
    {
        IsSidebarExpanded = !IsSidebarExpanded;
    }

    [RelayCommand]
    public void SwitchLanguage(string locale)
    {
        CurrentLocale = locale;
        LocalizationManager.Instance.CurrentLocale = locale;
    }

    [RelayCommand]
    public void MinimizeWindow(Window window)
    {
        if (window != null)
        {
            window.WindowState = WindowState.Minimized;
        }
    }

    [RelayCommand]
    public void ToggleMaximize(Window window)
    {
        if (window != null)
        {
            window.WindowState = window.WindowState == WindowState.Maximized ? WindowState.Normal : WindowState.Maximized;
        }
    }

    [RelayCommand]
    public void CloseToTray(Window window)
    {
        if (window != null)
        {
            window.Hide();
        }
    }
}
