using System;
using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Prism.ViewModels;

namespace Prism.Views;

public partial class ClientLogsView : UserControl
{
    private ListBox? _logsList;

    public ClientLogsView()
    {
        InitializeComponent();
        _logsList = this.FindControl<ListBox>("LogsList");
        DataContextChanged += OnDataContextChanged;
    }

    private void OnDataContextChanged(object? sender, EventArgs e)
    {
        if (DataContext is ClientLogsViewModel vm)
        {
            vm.FilteredLogs.CollectionChanged += (s, args) =>
            {
                if (vm.AutoScroll && vm.FilteredLogs.Count > 0 && _logsList != null)
                {
                    Avalonia.Threading.Dispatcher.UIThread.Post(() =>
                    {
                        if (vm.FilteredLogs.Count > 0)
                        {
                            _logsList.ScrollIntoView(vm.FilteredLogs.Count - 1);
                        }
                    });
                }
            };
        }
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
