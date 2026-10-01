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
    private string _statusCountText = "0 / 0";

    public ObservableCollection<ClientLogEntry> FilteredLogs { get; } = new();

    public ClientLogsViewModel()
    {
        _client.LogsUpdated += OnLogsUpdated;
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
                l.Target.Contains(SearchQuery, StringComparison.OrdinalIgnoreCase));
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
                // No new logs, do nothing
            }
            else
            {
                // Find matching index of the last known item in list
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
                    while (FilteredLogs.Count > 300)
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

    [RelayCommand]
    public void SetFilter(string level)
    {
        FilterLevel = level;
    }

    [RelayCommand]
    public async Task ClearLogsAsync()
    {
        await _client.ClearLogsAsync();
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
        }
    }
}
