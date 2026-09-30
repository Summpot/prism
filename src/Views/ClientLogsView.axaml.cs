using Avalonia.Controls;
using Avalonia.Markup.Xaml;

namespace Prism.Views;

public partial class ClientLogsView : UserControl
{
    public ClientLogsView()
    {
        InitializeComponent();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }
}
