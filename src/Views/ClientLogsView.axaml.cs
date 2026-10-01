using System;
using System.Collections.Specialized;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Prism.ViewModels;

namespace Prism.Views;

public partial class ClientLogsView : UserControl
{
    private ListBox? _logsList;
    private ClientLogsViewModel? _currentVm;

    public ClientLogsView()
    {
        InitializeComponent();
        _logsList = this.FindControl<ListBox>("LogsList");
        _logsList?.AddHandler(ScrollViewer.ScrollChangedEvent, OnLogsScrollChanged);
        DataContextChanged += OnDataContextChanged;
    }

    private void OnLogsScrollChanged(object? sender, ScrollChangedEventArgs e)
    {
        if (e.Source is ScrollViewer sv && _currentVm != null)
        {
            double remaining = sv.Extent.Height - (sv.Offset.Y + sv.Viewport.Height);
            bool isAtBottom = remaining <= 24 || sv.Extent.Height <= sv.Viewport.Height;
            _currentVm.IsAtBottom = isAtBottom;
        }
    }

    private void OnScrollToBottomRequested()
    {
        if (_logsList != null && _currentVm != null && _currentVm.FilteredLogs.Count > 0)
        {
            Avalonia.Threading.Dispatcher.UIThread.Post(() =>
            {
                if (_logsList != null && _currentVm != null && _currentVm.FilteredLogs.Count > 0)
                {
                    _logsList.ScrollIntoView(_currentVm.FilteredLogs[^1]);
                }
            });
        }
    }

    private void OnDataContextChanged(object? sender, EventArgs e)
    {
        if (_currentVm != null)
        {
            _currentVm.FilteredLogs.CollectionChanged -= OnFilteredLogsChanged;
            _currentVm.ScrollToBottomRequested -= OnScrollToBottomRequested;
        }

        _currentVm = DataContext as ClientLogsViewModel;

        if (_currentVm != null)
        {
            _currentVm.FilteredLogs.CollectionChanged += OnFilteredLogsChanged;
            _currentVm.ScrollToBottomRequested += OnScrollToBottomRequested;
        }
    }

    private void OnFilteredLogsChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        if (_currentVm != null && _currentVm.AutoScroll && _currentVm.FilteredLogs.Count > 0 && _logsList != null)
        {
            Avalonia.Threading.Dispatcher.UIThread.Post(() =>
            {
                if (_currentVm != null && _currentVm.AutoScroll && _currentVm.FilteredLogs.Count > 0 && _logsList != null)
                {
                    _logsList.ScrollIntoView(_currentVm.FilteredLogs[^1]);
                }
            });
        }
    }

    protected override void OnDetachedFromVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnDetachedFromVisualTree(e);
        if (_logsList != null)
        {
            _logsList.RemoveHandler(ScrollViewer.ScrollChangedEvent, OnLogsScrollChanged);
        }
        if (_currentVm != null)
        {
            _currentVm.FilteredLogs.CollectionChanged -= OnFilteredLogsChanged;
            _currentVm.ScrollToBottomRequested -= OnScrollToBottomRequested;
            _currentVm = null;
        }
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
