using System;
using System.Threading.Tasks;
using Avalonia.Threading;
using Prism.I18n;
using ShadUI;

namespace Prism.Services;

public static class AppServices
{
    public static DialogManager DialogManager { get; } = new();
    public static ToastManager ToastManager { get; } = new();

    public static void ShowSuccess(string message, string? title = null)
    {
        Show(title ?? I18nText.T("common_success", "Success"), message, 2.5, Notification.Success);
    }

    public static void ShowError(string message, string? title = null)
    {
        Show(title ?? I18nText.T("common_error", "Error"), message, 4.0, Notification.Error);
    }

    public static void ShowInfo(string message, string? title = null)
    {
        Show(title ?? I18nText.T("common_info", "Notice"), message, 2.5, Notification.Info);
    }

    public static void ShowWarning(string message, string? title = null)
    {
        Show(title ?? I18nText.T("common_warning", "Warning"), message, 3.5, Notification.Warning);
    }

    private static void Show(string title, string message, double delaySeconds, Notification kind)
    {
        Dispatcher.UIThread.Post(() =>
        {
            try
            {
                var toast = ToastManager.CreateToast(title);
                if (!string.IsNullOrWhiteSpace(message))
                {
                    toast.WithContent(message);
                }
                toast.WithDelay(delaySeconds).Show(kind);
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Toast] Error: {ex.Message}");
            }
        });
    }

    public static Task<bool> ConfirmAsync(
        string title,
        string message,
        string? confirmText = null,
        bool destructive = false)
    {
        var tcs = new TaskCompletionSource<bool>();
        Dispatcher.UIThread.Post(() =>
        {
            try
            {
                var primaryStyle = destructive ? DialogButtonStyle.Destructive : DialogButtonStyle.Primary;
                DialogManager
                    .CreateDialog(title, message)
                    .WithPrimaryButton(
                        confirmText ?? I18nText.T("common_confirm", "Confirm"),
                        () => tcs.TrySetResult(true),
                        primaryStyle)
                    .WithCancelButton(
                        I18nText.T("common_cancel", "Cancel"),
                        () => tcs.TrySetResult(false))
                    .Show();
            }
            catch (Exception ex)
            {
                Console.WriteLine($"[Dialog] Error: {ex.Message}");
                tcs.TrySetResult(false);
            }
        });
        return tcs.Task;
    }
}
