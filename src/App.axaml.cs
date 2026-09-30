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

            var showItem = new NativeMenuItem("打开主界面");
            showItem.Click += (s, e) => ToggleMainWindow(mainWindow);
            menu.Items.Add(showItem);

            _toggleTunnelItem = new NativeMenuItem("启动连接");
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

            var exitItem = new NativeMenuItem("退出 Prism");
            exitItem.Click += (s, e) => ExitApp(desktop);
            menu.Items.Add(exitItem);

            _trayIcon.Menu = menu;
            _trayIcon.Clicked += (s, e) => ToggleMainWindow(mainWindow);

            trayIcons.Add(_trayIcon);

            // React to status changes
            NativeClientService.Instance.StatusUpdated += status =>
            {
                Dispatcher.UIThread.Post(() =>
                {
                    if (_trayIcon != null)
                    {
                        _trayIcon.ToolTipText = status.Running
                            ? $"Prism - 运行中 ({status.ServerAddr})"
                            : "Prism - 空闲";
                    }
                    if (_toggleTunnelItem != null)
                    {
                        _toggleTunnelItem.Header = status.Running ? "断开连接" : "启动连接";
                    }
                });
            };
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
                if (a.StartsWith("prism://", StringComparison.OrdinalIgnoreCase))
                {
                    DesktopService.TriggerDeepLink(a);
                }
            }
        }
    }
}
