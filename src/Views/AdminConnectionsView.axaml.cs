using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Prism.Views;

public partial class AdminConnectionsView : UserControl
{
    public AdminConnectionsView()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
