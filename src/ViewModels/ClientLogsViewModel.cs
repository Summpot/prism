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

public partial class ClientLogsViewModel : ViewModelBase
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
        FilteredLogs.Clear();
        foreach (var entry in list)
        {
            FilteredLogs.Add(entry);
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
