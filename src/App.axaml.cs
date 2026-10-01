using System;
using System.Linq;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Controls.ApplicationLifetimes;
using Avalonia.Markup.Xaml;
using Avalonia.Platform;
using Avalonia.Threading;
using Prism.I18n;
using Prism.Services;
using Prism.Views;

namespace Prism;

public partial class App : Application
{
    private static App? s_current;
    private TrayIcon? _trayIcon;
    private NativeMenuItem? _toggleTunnelItem;
    private NativeMenuItem? _hideItem;
    private bool _isExplicitExit;
    private bool _trayReady;
    private string? _dismissedUpdateVersion;

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
        LocalizationManager.Instance.CurrentLocale = DesktopService.GetUiLocale();

        if (ApplicationLifetime is IClassicDesktopStyleApplicationLifetime desktop)
        {
            var mainWindow = new MainWindow
            {
                DataContext = new ViewModels.MainWindowViewModel()
            };
            desktop.MainWindow = mainWindow;

            _trayReady = SetupTrayIcon(desktop, mainWindow);

            mainWindow.Closing += (sender, e) =>
            {
                if (!_isExplicitExit && _trayReady)
                {
                    e.Cancel = true;
                    mainWindow.Hide();
                }
            };

            var args = desktop.Args ?? Array.Empty<string>();
            bool isSilent = args.Any(a => a == "--silent" || a == "--minimized" || a == "--autostart");
            if (isSilent)
            {
                if (_trayReady)
                {
                    mainWindow.Hide();
                }
                else
                {
                    mainWindow.WindowState = WindowState.Minimized;
                    mainWindow.Show();
                }
            }
            else
            {
                mainWindow.Show();
            }

            foreach (var arg in args)
            {
                if (arg.StartsWith("prism://", StringComparison.OrdinalIgnoreCase))
                {
                    DesktopService.TriggerDeepLink(arg);
                }
            }

            _ = RunStartupLifecycleAsync();
        }

        base.OnFrameworkInitializationCompleted();
    }

    private async Task RunStartupLifecycleAsync()
    {
        try
        {
            var client = NativeClientService.Instance;
            var cfg = client.GetConfig();

            if (cfg.ActiveConfig.AutoConnect && !string.IsNullOrWhiteSpace(cfg.ActiveConfig.ServerAddr))
            {
                try
                {
                    await client.StartAsync();
                }
                catch (Exception ex)
                {
                    Console.WriteLine($"[WARN] Auto-connect failed: {ex.Message}");
                    Dispatcher.UIThread.Post(() =>
                        AppServices.ShowError(ex.Message, I18nText.T("client_connection_failed", "Connection failed")));
                }
            }

            if (cfg.ActiveConfig.AutoCheckUpdate)
            {
                await Task.Delay(4000);
                await CheckUpdateOnStartupAsync(cfg.ActiveConfig.UpdateChannel);
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[WARN] Startup lifecycle error: {ex.Message}");
        }
    }

    private async Task CheckUpdateOnStartupAsync(string? channel)
    {
        try
        {
            var res = await NativeClientService.Instance.CheckUpdateAsync(channel);
            if (!res.Available || string.IsNullOrWhiteSpace(res.Version)) return;
            if (string.Equals(_dismissedUpdateVersion, res.Version, StringComparison.Ordinal)) return;

            string title = I18nText.T("client_update_prompt_title", "New Version Available");
            string body = I18nText.Format("client_update_prompt_desc", ("version", res.Version));
            if (!string.IsNullOrWhiteSpace(res.Body))
            {
                body = body + "\n\n" + res.Body;
            }

            bool confirm = await AppServices.ConfirmAsync(
                title,
                body,
                I18nText.T("client_update_prompt_confirm", "Update & Restart"));

            if (!confirm)
            {
                _dismissedUpdateVersion = res.Version;
                return;
            }

            try
            {
                await NativeClientService.Instance.InstallUpdateAsync(channel);
            }
            catch (Exception installEx)
            {
                AppServices.ShowError(
                    I18nText.Format("client_update_failed", ("error", installEx.Message)),
                    I18nText.T("client_update_prompt_title"));
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[WARN] Background update check failed: {ex.Message}");
        }
    }

    private bool SetupTrayIcon(IClassicDesktopStyleApplicationLifetime desktop, MainWindow mainWindow)
    {
        try
        {
            var trayIcons = TrayIcon.GetIcons(this);
            if (trayIcons == null)
            {
                Console.WriteLine("[WARN] TrayIcon.Icons is not declared; close-to-tray disabled.");
                return false;
            }

            var uri = new Uri("avares://Prism/Assets/favicon.ico");
            var iconStream = AssetLoader.Open(uri);
            var windowIcon = new WindowIcon(iconStream);

            _trayIcon = new TrayIcon
            {
                Icon = windowIcon,
                ToolTipText = "Prism"
            };

            var menu = new NativeMenu();

            var showItem = new NativeMenuItem(I18nText.T("tray_open"));
            showItem.Click += (_, _) =>
            {
                mainWindow.Show();
                mainWindow.WindowState = WindowState.Normal;
                mainWindow.Activate();
            };
            menu.Items.Add(showItem);

            _hideItem = new NativeMenuItem(I18nText.T("tray_hide"));
            _hideItem.Click += (_, _) => mainWindow.Hide();
            menu.Items.Add(_hideItem);

            _toggleTunnelItem = new NativeMenuItem(I18nText.T("tray_connect"));
            _toggleTunnelItem.Click += async (_, _) =>
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
                    AppServices.ShowError(ex.Message);
                }
            };
            menu.Items.Add(_toggleTunnelItem);

            menu.Items.Add(new NativeMenuItemSeparator());

            var exitItem = new NativeMenuItem(I18nText.T("tray_exit"));
            exitItem.Click += (_, _) => ExitApp(desktop);
            menu.Items.Add(exitItem);

            _trayIcon.Menu = menu;
            _trayIcon.Clicked += (_, _) => ToggleMainWindow(mainWindow);

            trayIcons.Add(_trayIcon);

            void UpdateTrayTexts()
            {
                var status = NativeClientService.Instance.CurrentStatus;
                bool isRunning = status?.Running == true;
                showItem.Header = I18nText.T("tray_open");
                exitItem.Header = I18nText.T("tray_exit");
                if (_hideItem != null)
                {
                    _hideItem.Header = I18nText.T("tray_hide");
                }
                if (_toggleTunnelItem != null)
                {
                    _toggleTunnelItem.Header = isRunning
                        ? I18nText.T("tray_disconnect")
                        : I18nText.T("tray_connect");
                }
                if (_trayIcon != null)
                {
                    _trayIcon.ToolTipText = isRunning
                        ? I18nText.Format("tray_running", ("0", status?.ServerAddr ?? ""))
                        : I18nText.T("tray_idle");
                }
            }

            UpdateTrayTexts();
            Messages.CurrentLocaleChanged += () => Dispatcher.UIThread.Post(UpdateTrayTexts);
            NativeClientService.Instance.StatusUpdated += _ => Dispatcher.UIThread.Post(UpdateTrayTexts);
            return true;
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[WARN] Failed to setup tray icon: {ex.Message}");
            return false;
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
                if (a.StartsWith("prism://", StringComparison.OrdinalIgnoreCase) ||
                    a.Contains("://", StringComparison.Ordinal) && PrismLinkService.ParseDeepLink(a).Kind != "unknown")
                {
                    DesktopService.TriggerDeepLink(a);
                }
            }
        }
    }
}
