using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Prism.ViewModels;

namespace Prism.Views;

public partial class MainWindow : ShadUI.Window
{
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
        if (DataContext is MainWindowViewModel vm && e.NewSize.Width < 960)
        {
            vm.IsSidebarExpanded = false;
        }
    }
}
