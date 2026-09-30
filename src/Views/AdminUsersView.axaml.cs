using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Prism.Views;

public partial class AdminUsersView : UserControl
{
    public AdminUsersView()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
