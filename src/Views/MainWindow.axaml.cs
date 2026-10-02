using Avalonia.Controls;
using Avalonia.Controls.Presenters;
using Avalonia.Controls.Primitives;
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

    protected override void OnApplyTemplate(TemplateAppliedEventArgs e)
    {
        base.OnApplyTemplate(e);

        if (e.NameScope.Find<StackPanel>("AppTitlePanel") is { } titlePanel)
        {
            titlePanel.IsHitTestVisible = true;
            foreach (var child in titlePanel.Children)
            {
                if (child is ContentPresenter cp)
                {
                    cp.IsHitTestVisible = true;
                }
                else if (child is TextBlock tb)
                {
                    tb.IsHitTestVisible = false;
                }
            }
        }
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
