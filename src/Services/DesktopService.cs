using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Text;

namespace Prism.Services;

public static class DesktopService
{
    private static readonly object Sync = new();
    private static readonly List<string> PendingDeepLinks = new();
    private static readonly HashSet<string> ConsumedDeepLinks = new(StringComparer.OrdinalIgnoreCase);
    private static Action<string>? _deepLinkReceived;

    public static string? PendingOAuthState { get; set; }

    public static event Action<string>? DeepLinkReceived
    {
        add
        {
            _deepLinkReceived += value;
            List<string> replay;
            lock (Sync)
            {
                replay = [.. PendingDeepLinks];
                PendingDeepLinks.Clear();
            }
            foreach (var uri in replay)
            {
                value?.Invoke(uri);
            }
        }
        remove => _deepLinkReceived -= value;
    }

    public static void TriggerDeepLink(string uri)
    {
        if (string.IsNullOrWhiteSpace(uri)) return;
        uri = uri.Trim();

        Action<string>? handler;
        lock (Sync)
        {
            if (!ConsumedDeepLinks.Add(uri)) return;
            handler = _deepLinkReceived;
            if (handler == null)
            {
                PendingDeepLinks.Add(uri);
                return;
            }
        }
        handler.Invoke(uri);
    }

    public static void RegisterPrismProtocol()
    {
        try
        {
            string? exePath = Environment.ProcessPath;
            if (string.IsNullOrEmpty(exePath)) return;

            if (OperatingSystem.IsWindows())
            {
                RegisterWindowsProtocol(exePath);
            }
            else if (OperatingSystem.IsLinux())
            {
                RegisterLinuxProtocol(exePath);
            }
            else if (OperatingSystem.IsMacOS())
            {
                RegisterMacOsProtocol(exePath);
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[WARN] Failed to register prism:// protocol: {ex.Message}");
        }
    }

    private static void RegisterWindowsProtocol(string exePath)
    {
        using var key = Microsoft.Win32.Registry.CurrentUser.CreateSubKey(@"Software\Classes\prism");
        if (key == null) return;
        key.SetValue("", "URL:Prism Protocol");
        key.SetValue("URL Protocol", "");
        using var defaultIcon = key.CreateSubKey("DefaultIcon");
        defaultIcon?.SetValue("", $"\"{exePath}\",0");
        using var shell = key.CreateSubKey(@"shell\open\command");
        shell?.SetValue("", $"\"{exePath}\" \"%1\"");
    }

    private static void RegisterLinuxProtocol(string exePath)
    {
        string appsDir = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            ".local", "share", "applications");
        Directory.CreateDirectory(appsDir);
        string desktopPath = Path.Combine(appsDir, "prism.desktop");
        var desktop = new StringBuilder();
        desktop.AppendLine("[Desktop Entry]");
        desktop.AppendLine("Name=Prism");
        desktop.AppendLine("Comment=Prism tunnel client");
        desktop.AppendLine($"Exec=\"{exePath}\" %u");
        desktop.AppendLine("Terminal=false");
        desktop.AppendLine("Type=Application");
        desktop.AppendLine("Categories=Network;");
        desktop.AppendLine("MimeType=x-scheme-handler/prism;");
        File.WriteAllText(desktopPath, desktop.ToString());
        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = "xdg-mime",
                Arguments = "default prism.desktop x-scheme-handler/prism",
                UseShellExecute = false,
                CreateNoWindow = true
            })?.Dispose();
        }
        catch { }
    }

    [System.Runtime.Versioning.SupportedOSPlatform("macos")]
    private static void RegisterMacOsProtocol(string exePath)
    {
        string support = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            "Library", "Application Support", "Prism");
        string app = Path.Combine(support, "prism-url-handler.app");
        string contents = Path.Combine(app, "Contents");
        string macos = Path.Combine(contents, "MacOS");
        Directory.CreateDirectory(macos);

        string plist =
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n" +
            "<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n" +
            "<plist version=\"1.0\"><dict>\n" +
            "<key>CFBundleIdentifier</key><string>dev.prism.url-handler</string>\n" +
            "<key>CFBundleName</key><string>Prism URL Handler</string>\n" +
            "<key>CFBundleExecutable</key><string>handler</string>\n" +
            "<key>CFBundlePackageType</key><string>APPL</string>\n" +
            "<key>CFBundleVersion</key><string>1</string>\n" +
            "<key>CFBundleShortVersionString</key><string>1.0</string>\n" +
            "<key>CFBundleURLTypes</key><array><dict>\n" +
            "<key>CFBundleURLName</key><string>Prism</string>\n" +
            "<key>CFBundleURLSchemes</key><array><string>prism</string></array>\n" +
            "</dict></array>\n" +
            "</dict></plist>\n";
        File.WriteAllText(Path.Combine(contents, "Info.plist"), plist);

        string quoted = "'" + exePath.Replace("'", "'\\''") + "'";
        string handler = Path.Combine(macos, "handler");
        File.WriteAllText(handler, "#!/bin/sh\nexec " + quoted + " \"$@\"\n");
        File.SetUnixFileMode(
            handler,
            UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute |
            UnixFileMode.GroupRead | UnixFileMode.GroupExecute |
            UnixFileMode.OtherRead | UnixFileMode.OtherExecute);

        try
        {
            Process.Start(new ProcessStartInfo
            {
                FileName = "/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister",
                ArgumentList = { "-f", app },
                UseShellExecute = false,
                CreateNoWindow = true
            })?.Dispose();
        }
        catch { }
    }

    public static bool IsAutostartEnabled()
    {
        try
        {
            if (OperatingSystem.IsWindows())
            {
                using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run", false);
                return key?.GetValue("Prism") != null;
            }
            if (OperatingSystem.IsLinux())
            {
                return File.Exists(LinuxAutostartPath());
            }
            if (OperatingSystem.IsMacOS())
            {
                return File.Exists(MacLaunchAgentPath());
            }
        }
        catch { }
        return false;
    }

    public static void SetAutostart(bool enable, bool silent = false)
    {
        try
        {
            string? exePath = Environment.ProcessPath;
            if (string.IsNullOrEmpty(exePath)) return;

            if (OperatingSystem.IsWindows())
            {
                using var key = Microsoft.Win32.Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run", true);
                if (key == null) return;
                if (enable)
                {
                    string cmd = silent ? $"\"{exePath}\" --silent" : $"\"{exePath}\"";
                    key.SetValue("Prism", cmd);
                }
                else
                {
                    key.DeleteValue("Prism", false);
                }
                return;
            }

            if (OperatingSystem.IsLinux())
            {
                string path = LinuxAutostartPath();
                if (enable)
                {
                    Directory.CreateDirectory(Path.GetDirectoryName(path)!);
                    string args = silent ? " --silent" : "";
                    var desktop = new StringBuilder();
                    desktop.AppendLine("[Desktop Entry]");
                    desktop.AppendLine("Type=Application");
                    desktop.AppendLine("Name=Prism");
                    desktop.AppendLine($"Exec=\"{exePath}\"{args}");
                    desktop.AppendLine("X-GNOME-Autostart-enabled=true");
                    File.WriteAllText(path, desktop.ToString());
                }
                else if (File.Exists(path))
                {
                    File.Delete(path);
                }
                return;
            }

            if (OperatingSystem.IsMacOS())
            {
                string path = MacLaunchAgentPath();
                if (enable)
                {
                    Directory.CreateDirectory(Path.GetDirectoryName(path)!);
                    string argsXml = silent
                        ? "\n    <string>--silent</string>"
                        : "";
                    var plist = $"""
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>xyz.prism.desktop</string>
    <key>ProgramArguments</key>
    <array>
    <string>{exePath}</string>{argsXml}
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
""";
                    File.WriteAllText(path, plist);
                }
                else if (File.Exists(path))
                {
                    File.Delete(path);
                }
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[WARN] Failed to set autostart: {ex.Message}");
        }
    }

    private static string LinuxAutostartPath() =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), ".config", "autostart", "prism.desktop");

    private static string MacLaunchAgentPath() =>
        Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile), "Library", "LaunchAgents", "xyz.prism.desktop.plist");

    private static readonly string SettingsFilePath = Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
        "prism",
        "desktop_ui.json");

    public static string GetUiTheme()
    {
        try
        {
            if (File.Exists(SettingsFilePath))
            {
                var text = File.ReadAllText(SettingsFilePath);
                var match = System.Text.RegularExpressions.Regex.Match(text, @"""theme""\s*:\s*""([^""]+)""");
                if (match.Success) return match.Groups[1].Value;
            }
        }
        catch { }
        return "Default";
    }

    public static void SetUiTheme(string theme)
    {
        SaveUiSetting("theme", theme);
    }

    public static string GetUiLocale()
    {
        try
        {
            if (File.Exists(SettingsFilePath))
            {
                var text = File.ReadAllText(SettingsFilePath);
                var match = System.Text.RegularExpressions.Regex.Match(text, @"""locale""\s*:\s*""([^""]+)""");
                if (match.Success) return match.Groups[1].Value;
            }
        }
        catch { }
        return "zh-CN";
    }

    public static void SetUiLocale(string locale)
    {
        SaveUiSetting("locale", locale);
    }

    private static void SaveUiSetting(string key, string value)
    {
        try
        {
            var dir = Path.GetDirectoryName(SettingsFilePath);
            if (!string.IsNullOrEmpty(dir) && !Directory.Exists(dir))
            {
                Directory.CreateDirectory(dir);
            }
            string theme = GetUiTheme();
            string locale = GetUiLocale();
            if (key == "theme") theme = value;
            if (key == "locale") locale = value;

            var json = $"{{\n  \"theme\": \"{theme}\",\n  \"locale\": \"{locale}\"\n}}";
            File.WriteAllText(SettingsFilePath, json);
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[WARN] Failed to save desktop UI setting: {ex.Message}");
        }
    }
}
