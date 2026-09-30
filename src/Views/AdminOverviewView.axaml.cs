using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Prism.Views;

public partial class AdminOverviewView : UserControl
{
    public AdminOverviewView()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
