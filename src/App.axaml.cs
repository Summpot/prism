using System;
using System.Linq;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Avalonia.Platform;
using Avalonia.Threading;
using Prism.Services;
using Prism.Views;

namespace Prism;

public partial class App : Application
{
    private static App? s_current;
    private TrayIcon? _trayIcon;
    private NativeMenuItem? _toggleTunnelItem;
    private bool _isExplicitExit;

    public override void Initialize()
    {
        s_current = this;
        AvaloniaXamlLoader.Load(this);
    }

    public override void OnFrameworkInitializationCompleted()
    {
        string savedTheme = DesktopService.GetUiTheme();
        RequestedThemeVariant = savedTheme switch
        {
            "Light" => Avalonia.Styling.ThemeVariant.Light,
            "Dark" => Avalonia.Styling.ThemeVariant.Dark,
            _ => Avalonia.Styling.ThemeVariant.Default
        };
        I18n.LocalizationManager.Instance.CurrentLocale = DesktopService.GetUiLocale();

        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var mainWindow = new MainWindow
            {
                DataContext = new ViewModels.MainWindowViewModel()
            };
            desktop.MainWindow = mainWindow;

            // Close-to-tray handler: hide instead of terminate
            mainWindow.Closing += (sender, e) =>
            {
                if (!_isExplicitExit)
                {
                    e.Cancel = true;
                    mainWindow.Hide();
                }
            };

            SetupTrayIcon(desktop, mainWindow);

            // Handle start arguments (--silent, --minimized, deep link)
            var args = desktop.Args ?? Array.Empty<string>();
            bool isSilent = args.Any(a => a == "--silent" || a == "--minimized" || a == "--autostart");
            if (isSilent)
            {
                mainWindow.WindowState = WindowState.Minimized;
            }
            else
            {
                mainWindow.Show();
            }

            // Check initial deep links in args
            foreach (var arg in args)
            {
                if (arg.StartsWith("prism://", StringComparison.OrdinalIgnoreCase))
                {
                    DesktopService.TriggerDeepLink(arg);
                }
            }
        }

        base.OnFrameworkInitializationCompleted();
    }

    private void SetupTrayIcon(IClassicDesktopStyleApplicationLifetime desktop, MainWindow mainWindow)
    {
        try
        {
            var trayIcons = TrayIcon.GetIcons(this);
            if (trayIcons == null) return;

            var uri = new Uri("avares://Prism/Assets/favicon.ico");
            var iconStream = AssetLoader.Open(uri);
            var windowIcon = new WindowIcon(iconStream);

            _trayIcon = new TrayIcon
            {
                Icon = windowIcon,
                ToolTipText = "Prism"
            };

            var menu = new NativeMenu();

            var showItem = new NativeMenuItem(I18n.LocalizationManager.Instance["tray_open"]);
            showItem.Click += (s, e) => ToggleMainWindow(mainWindow);
            menu.Items.Add(showItem);

            _toggleTunnelItem = new NativeMenuItem(I18n.LocalizationManager.Instance["tray_connect"]);
            _toggleTunnelItem.Click += async (s, e) =>
            {
                try
                {
                    var client = NativeClientService.Instance;
                    if (client.CurrentStatus?.Running == true)
                    {
                        await client.StopAsync();
                    }
                    else
                    {
                        await client.StartAsync();
                    }
                }
                catch (Exception ex)
                {
                    Console.WriteLine($"[TRAY] Toggle tunnel error: {ex.Message}");
                }
            };
            menu.Items.Add(_toggleTunnelItem);

            menu.Items.Add(new NativeMenuItemSeparator());

            var exitItem = new NativeMenuItem(I18n.LocalizationManager.Instance["tray_exit"]);
            exitItem.Click += (s, e) => ExitApp(desktop);
            menu.Items.Add(exitItem);

            _trayIcon.Menu = menu;
            _trayIcon.Clicked += (s, e) => ToggleMainWindow(mainWindow);

            trayIcons.Add(_trayIcon);

            Action updateTrayTexts = () =>
            {
                var status = NativeClientService.Instance.CurrentStatus;
                bool isRunning = status?.Running == true;
                showItem.Header = I18n.LocalizationManager.Instance["tray_open"];
                exitItem.Header = I18n.LocalizationManager.Instance["tray_exit"];
                if (_toggleTunnelItem != null)
                {
                    _toggleTunnelItem.Header = isRunning
                        ? I18n.LocalizationManager.Instance["tray_disconnect"]
                        : I18n.LocalizationManager.Instance["tray_connect"];
                }
                if (_trayIcon != null)
                {
                    _trayIcon.ToolTipText = isRunning
                        ? string.Format(I18n.LocalizationManager.Instance["tray_running"], status?.ServerAddr ?? "")
                        : I18n.LocalizationManager.Instance["tray_idle"];
                }
            };

            updateTrayTexts();
            I18n.Messages.CurrentLocaleChanged += () => Dispatcher.UIThread.Post(updateTrayTexts);
            NativeClientService.Instance.StatusUpdated += _ => Dispatcher.UIThread.Post(updateTrayTexts);
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[WARN] Failed to setup tray icon: {ex.Message}");
        }
    }

    public static void ToggleMainWindow(Window window)
    {
        if (window.IsVisible)
        {
            window.Hide();
        }
        else
        {
            window.Show();
            window.WindowState = WindowState.Normal;
            window.Activate();
        }
    }

    public void ExitApp(IClassicDesktopStyleApplicationLifetime desktop)
    {
        _isExplicitExit = true;
        _trayIcon?.Dispose();
        desktop.Shutdown();
    }

    public static void HandleIncomingArgs(string[] args)
    {
        if (s_current?.ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop && desktop.MainWindow != null)
        {
            if (!desktop.MainWindow.IsVisible)
            {
                desktop.MainWindow.Show();
            }
            desktop.MainWindow.WindowState = WindowState.Normal;
            desktop.MainWindow.Activate();

            foreach (var a in args)
            {
                if (a.StartsWith("prism://", StringComparison.OrdinalIgnoreCase) || PrismLinkService.Parse(a) != null)
                {
                    DesktopService.TriggerDeepLink(a);
                }
            }
        }
    }
}
