using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32;

namespace Prism.Services;

public static class DesktopService
{
    public static event Action<string>? DeepLinkReceived;

    public static void TriggerDeepLink(string uri)
    {
        if (string.IsNullOrWhiteSpace(uri)) return;
        DeepLinkReceived?.Invoke(uri.Trim());
    }

    public static void RegisterPrismProtocol()
    {
        if (!OperatingSystem.IsWindows()) return;

        try
        {
            string? exePath = Environment.ProcessPath;
            if (string.IsNullOrEmpty(exePath)) return;

            using var key = Registry.CurrentUser.CreateSubKey(@"Software\Classes\prism");
            if (key != null)
            {
                key.SetValue("", "URL:Prism Protocol");
                key.SetValue("URL Protocol", "");

                using var defaultIcon = key.CreateSubKey("DefaultIcon");
                defaultIcon?.SetValue("", $"\"{exePath}\",0");

                using var shell = key.CreateSubKey(@"shell\open\command");
                shell?.SetValue("", $"\"{exePath}\" \"%1\"");
            }
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[WARN] Failed to register prism:// protocol: {ex.Message}");
        }
    }

    public static bool IsAutostartEnabled()
    {
        if (!OperatingSystem.IsWindows()) return false;

        try
        {
            using var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run", false);
            return key?.GetValue("Prism") != null;
        }
        catch
        {
            return false;
        }
    }

    public static void SetAutostart(bool enable, bool silent = false)
    {
        if (!OperatingSystem.IsWindows()) return;

        try
        {
            string? exePath = Environment.ProcessPath;
            if (string.IsNullOrEmpty(exePath)) return;

            using var key = Registry.CurrentUser.OpenSubKey(@"Software\Microsoft\Windows\CurrentVersion\Run", true);
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
        }
        catch (Exception ex)
        {
            Console.WriteLine($"[WARN] Failed to set autostart: {ex.Message}");
        }
    }

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
