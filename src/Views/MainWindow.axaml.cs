using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Prism.ViewModels;

namespace Prism.Views;

public partial class MainWindow : ShadUI.Window
{
    private bool _sidebarSized;

    public MainWindow()
    {
        InitializeComponent();
        SizeChanged += OnWindowSizeChanged;
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    private void OnWindowSizeChanged(object? sender, SizeChangedEventArgs e)
    {
        if (_sidebarSized || e.NewSize.Width <= 0) return;
        if (DataContext is not MainWindowViewModel vm) return;
        _sidebarSized = true;
        if (e.NewSize.Width < 960)
        {
            vm.IsSidebarExpanded = false;
        }
    }
}
