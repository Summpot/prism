using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Prism.Views;

public partial class ClientTrafficView : UserControl
{
    public ClientTrafficView()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
