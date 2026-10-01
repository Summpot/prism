using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using Avalonia;
using Avalonia.Input.Platform;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.I18n;
using Prism.Native;
using Prism.Services;

namespace Prism.ViewModels;

public partial class ClientLogsViewModel : ViewModelBase, INavigationAware
{
    private readonly NativeClientService _client = NativeClientService.Instance;

    [ObservableProperty]
    private string _filterLevel = "ALL";

    [ObservableProperty]
    private string _searchQuery = "";

    [ObservableProperty]
    private bool _autoScroll = true;

    [ObservableProperty]
    private string _scrollButtonText = "滚动吸附：开启";

    [ObservableProperty]
    private string _statusCountText = "0 / 0";

    [ObservableProperty]
    private bool _hasLogs;

    [ObservableProperty]
    private bool _isEmpty = true;

    [ObservableProperty]
    private bool _copiedAll;

    [ObservableProperty]
    private string _copyButtonText = "复制全部";

    [ObservableProperty]
    private bool _isAtBottom = true;

    [ObservableProperty]
    private bool _showScrollToBottom;

    public event Action? ScrollToBottomRequested;

    public ObservableCollection<ClientLogEntry> FilteredLogs { get; } = new();

    public ClientLogsViewModel()
    {
        _client.LogsUpdated += OnLogsUpdated;
        UpdateScrollButtonText();
        CopyButtonText = LocalizationManager.Instance["client_logs_copy_all"] ?? "复制全部";
        OnLogsUpdated(_client.CurrentLogs);
    }

    public void OnNavigatedTo()
    {
        _ = _client.RefreshLogsAsync();
    }

    public void OnNavigatedFrom()
    {
    }

    private string _lastFilter = "";
    private string _lastSearch = "";

    private void UpdateScrollButtonText()
    {
        string stateStr = AutoScroll
            ? (IsAtBottom
                ? (LocalizationManager.Instance["client_logs_on"] ?? "开启")
                : (LocalizationManager.Instance["client_logs_paused"] ?? "暂停"))
            : (LocalizationManager.Instance["client_logs_off"] ?? "关闭");
        string template = LocalizationManager.Instance["client_logs_scroll"] ?? "滚动吸附：{state}";
        ScrollButtonText = template.Replace("{state}", stateStr);
    }

    private void OnLogsUpdated(List<ClientLogEntry> logs)
    {
        var filtered = logs.AsEnumerable();

        if (FilterLevel != "ALL")
        {
            filtered = filtered.Where(l => string.Equals(l.Level, FilterLevel, StringComparison.OrdinalIgnoreCase));
        }

        if (!string.IsNullOrWhiteSpace(SearchQuery))
        {
            filtered = filtered.Where(l =>
                l.Message.Contains(SearchQuery, StringComparison.OrdinalIgnoreCase) ||
                l.Target.Contains(SearchQuery, StringComparison.OrdinalIgnoreCase) ||
                l.Level.Contains(SearchQuery, StringComparison.OrdinalIgnoreCase));
        }

        var list = filtered.ToList();

        bool filterChanged = _lastFilter != FilterLevel || _lastSearch != SearchQuery;
        _lastFilter = FilterLevel;
        _lastSearch = SearchQuery;

        if (filterChanged || FilteredLogs.Count == 0 || list.Count == 0)
        {
            FilteredLogs.Clear();
            foreach (var entry in list)
            {
                FilteredLogs.Add(entry);
            }
        }
        else
        {
            var lastExisting = FilteredLogs[^1];
            var lastNew = list[^1];
            if (lastExisting.Timestamp == lastNew.Timestamp &&
                lastExisting.Message == lastNew.Message &&
                lastExisting.Target == lastNew.Target)
            {
                // No new logs
            }
            else
            {
                int matchIdx = -1;
                for (int i = list.Count - 1; i >= 0; i--)
                {
                    if (list[i].Timestamp == lastExisting.Timestamp &&
                        list[i].Message == lastExisting.Message &&
                        list[i].Target == lastExisting.Target)
                    {
                        matchIdx = i;
                        break;
                    }
                }

                if (matchIdx >= 0)
                {
                    for (int i = matchIdx + 1; i < list.Count; i++)
                    {
                        FilteredLogs.Add(list[i]);
                    }
                    while (FilteredLogs.Count > 400)
                    {
                        FilteredLogs.RemoveAt(0);
                    }
                }
                else
                {
                    FilteredLogs.Clear();
                    foreach (var entry in list)
                    {
                        FilteredLogs.Add(entry);
                    }
                }
            }
        }

        HasLogs = FilteredLogs.Count > 0;
        IsEmpty = !HasLogs;
        ShowScrollToBottom = !IsAtBottom && HasLogs;
        StatusCountText = $"{FilteredLogs.Count} / {logs.Count}";
    }

    partial void OnFilterLevelChanged(string value)
    {
        OnLogsUpdated(_client.CurrentLogs);
    }

    partial void OnSearchQueryChanged(string value)
    {
        OnLogsUpdated(_client.CurrentLogs);
    }

    partial void OnAutoScrollChanged(bool value)
    {
        UpdateScrollButtonText();
    }

    partial void OnIsAtBottomChanged(bool value)
    {
        UpdateScrollButtonText();
        ShowScrollToBottom = !value && FilteredLogs.Count > 0;
    }

    [RelayCommand]
    public void SetFilter(string level)
    {
        FilterLevel = level;
    }

    [RelayCommand]
    public void ToggleAutoScroll()
    {
        if (AutoScroll && IsAtBottom)
        {
            AutoScroll = false;
        }
        else
        {
            AutoScroll = true;
            ScrollToBottomRequested?.Invoke();
        }
    }

    [RelayCommand]
    public void ScrollToBottom()
    {
        AutoScroll = true;
        ScrollToBottomRequested?.Invoke();
    }

    [RelayCommand]
    public async Task ClearLogsAsync()
    {
        if (!await AppServices.ConfirmAsync(
                I18nText.T("common_confirm"),
                I18nText.T("confirm_clear_logs"),
                I18nText.T("client_logs_clear")))
        {
            return;
        }
        await _client.ClearLogsAsync();
        AppServices.ShowInfo(I18nText.T("logs_cleared"));
    }

    [RelayCommand]
    public async Task CopyAllLogsAsync()
    {
        if (Application.Current?.ApplicationLifetime is Avalonia.Controls.ApplicationLifetimes.IClassicDesktopStyleApplicationLifetime desktop &&
            desktop.MainWindow?.Clipboard != null)
        {
            var sb = new StringBuilder();
            foreach (var l in FilteredLogs)
            {
                sb.AppendLine($"[{l.Timestamp}] [{l.Level}] {l.Target}: {l.Message}");
            }
            await desktop.MainWindow.Clipboard.SetTextAsync(sb.ToString());
            CopiedAll = true;
            CopyButtonText = LocalizationManager.Instance["common_copied"] ?? "已复制";
            _ = Task.Delay(2000).ContinueWith(_ =>
            {
                Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                {
                    CopiedAll = false;
                    CopyButtonText = LocalizationManager.Instance["client_logs_copy_all"] ?? "复制全部";
                });
            });
        }
    }
}
