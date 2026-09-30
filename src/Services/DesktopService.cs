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
}
