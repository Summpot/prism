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
    private string _scrollButtonText = "";

    [ObservableProperty]
    private bool _isWordWrap = true;

    [ObservableProperty]
    private string _wrapButtonText = "";

    [ObservableProperty]
    private bool _hasLogs;

    [ObservableProperty]
    private bool _isEmpty = true;

    [ObservableProperty]
    private bool _copiedAll;

    [ObservableProperty]
    private string _copyButtonText = "";

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
        UpdateWrapButtonText();
        OnLogsUpdated(_client.CurrentLogs);
    }

    protected override void OnLocaleChanged()
    {
        base.OnLocaleChanged();
        UpdateScrollButtonText();
        UpdateWrapButtonText();
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
                ? I18nText.T("client_logs_on", "开启")
                : I18nText.T("client_logs_paused", "暂停"))
            : I18nText.T("client_logs_off", "关闭");
        string template = I18nText.T("client_logs_scroll", "滚动吸附：{state}");
        ScrollButtonText = template.Replace("{state}", stateStr);
        if (!CopiedAll)
        {
            CopyButtonText = I18nText.T("client_logs_copy_all", "复制全部");
        }
    }

    private void UpdateWrapButtonText()
    {
        string stateStr = IsWordWrap
            ? I18nText.T("client_logs_on", "开启")
            : I18nText.T("client_logs_off", "关闭");
        string template = I18nText.T("client_logs_wrap", "自动换行：{state}");
        WrapButtonText = template.Replace("{state}", stateStr);
    }

    private void OnLogsUpdated(List<ClientLogEntry> logs)
    {
        if (!Avalonia.Threading.Dispatcher.UIThread.CheckAccess())
        {
            Avalonia.Threading.Dispatcher.UIThread.Post(() => OnLogsUpdated(logs));
            return;
        }

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

    partial void OnIsWordWrapChanged(bool value)
    {
        UpdateWrapButtonText();
        if (AutoScroll && IsAtBottom)
        {
            ScrollToBottomRequested?.Invoke();
        }
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
    public void ToggleWordWrap()
    {
        IsWordWrap = !IsWordWrap;
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
            var logsSnapshot = FilteredLogs.ToList();
            string text = await Task.Run(() =>
            {
                var sb = new StringBuilder();
                foreach (var l in logsSnapshot)
                {
                    sb.AppendLine($"[{l.Timestamp}] [{l.Level}] {l.Target}: {l.Message}");
                }
                return sb.ToString();
            });

            await desktop.MainWindow.Clipboard.SetTextAsync(text);
            CopiedAll = true;
            CopyButtonText = I18nText.T("common_copied", "已复制");
            _ = Task.Delay(2000).ContinueWith(_ =>
            {
                Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                {
                    CopiedAll = false;
                    CopyButtonText = I18nText.T("client_logs_copy_all", "复制全部");
                });
            });
        }
    }
}
