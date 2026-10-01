using System;
using Avalonia.Threading;
using ShadUI;

namespace Prism.Services;

public static class AppServices
{
    public static DialogManager DialogManager { get; } = new();
    public static ToastManager ToastManager { get; } = new();

    public static void ShowSuccess(string message, string? title = "成功")
    {
        Dispatcher.UIThread.Post(() =>
        {
            try
            {
                var toast = ToastManager.CreateToast(title ?? "成功");
                if (!string.IsNullOrWhiteSpace(message))
                {
                    toast.WithContent(message);
                }
                toast.WithDelay(2500).Show();
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Toast] Error: {ex.Message}");
            }
        });
    }

    public static void ShowError(string message, string? title = "错误")
    {
        Dispatcher.UIThread.Post(() =>
        {
            try
            {
                var toast = ToastManager.CreateToast(title ?? "错误");
                if (!string.IsNullOrWhiteSpace(message))
                {
                    toast.WithContent(message);
                }
                toast.WithDelay(4000).Show();
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Toast] Error: {ex.Message}");
            }
        });
    }

    public static void ShowInfo(string message, string? title = "提示")
    {
        Dispatcher.UIThread.Post(() =>
        {
            try
            {
                var toast = ToastManager.CreateToast(title ?? "提示");
                if (!string.IsNullOrWhiteSpace(message))
                {
                    toast.WithContent(message);
                }
                toast.WithDelay(2500).Show();
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Toast] Error: {ex.Message}");
            }
        });
    }
}
