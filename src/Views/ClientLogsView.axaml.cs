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
        DataContextChanged += OnDataContextChanged;
    }

    private void OnDataContextChanged(object? sender, EventArgs e)
    {
        if (_currentVm != null)
        {
            _currentVm.FilteredLogs.CollectionChanged -= OnFilteredLogsChanged;
        }

        _currentVm = DataContext as ClientLogsViewModel;

        if (_currentVm != null)
        {
            _currentVm.FilteredLogs.CollectionChanged += OnFilteredLogsChanged;
        }
    }

    private void OnFilteredLogsChanged(object? sender, NotifyCollectionChangedEventArgs e)
    {
        if (_currentVm != null && _currentVm.AutoScroll && _currentVm.FilteredLogs.Count > 0 && _logsList != null)
        {
            Avalonia.Threading.Dispatcher.UIThread.Post(() =>
            {
                if (_currentVm != null && _currentVm.FilteredLogs.Count > 0 && _logsList != null)
                {
                    _logsList.ScrollIntoView(_currentVm.FilteredLogs[^1]);
                }
            });
        }
    }

    protected override void OnDetachedFromVisualTree(VisualTreeAttachmentEventArgs e)
    {
        base.OnDetachedFromVisualTree(e);
        if (_currentVm != null)
        {
            _currentVm.FilteredLogs.CollectionChanged -= OnFilteredLogsChanged;
            _currentVm = null;
        }
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
