using System.Windows.Input;
using Avalonia;
using Avalonia.Controls;
using Avalonia.Markup.Xaml;
using Prism.ViewModels;

namespace Prism.Views.Controls;

public partial class AdminReady : UserControl
{
    public static readonly StyledProperty<AdminReadyState> StateProperty =
        AvaloniaProperty.Register<AdminReady, AdminReadyState>(nameof(State), AdminReadyState.Loading);

    public static readonly StyledProperty<ICommand?> ConnectCommandProperty =
        AvaloniaProperty.Register<AdminReady, ICommand?>(nameof(ConnectCommand));

    public static readonly DirectProperty<AdminReady, bool> IsLoadingStateProperty =
        AvaloniaProperty.RegisterDirect<AdminReady, bool>(nameof(IsLoadingState), o => o.IsLoadingState);

    public static readonly DirectProperty<AdminReady, bool> IsDisconnectedStateProperty =
        AvaloniaProperty.RegisterDirect<AdminReady, bool>(nameof(IsDisconnectedState), o => o.IsDisconnectedState);

    public static readonly DirectProperty<AdminReady, bool> IsAccessDeniedStateProperty =
        AvaloniaProperty.RegisterDirect<AdminReady, bool>(nameof(IsAccessDeniedState), o => o.IsAccessDeniedState);

    private bool _isLoadingState = true;
    private bool _isDisconnectedState;
    private bool _isAccessDeniedState;

    public AdminReadyState State
    {
        get => GetValue(StateProperty);
        set => SetValue(StateProperty, value);
    }

    public ICommand? ConnectCommand
    {
        get => GetValue(ConnectCommandProperty);
        set => SetValue(ConnectCommandProperty, value);
    }

    public bool IsLoadingState
    {
        get => _isLoadingState;
        private set => SetAndRaise(IsLoadingStateProperty, ref _isLoadingState, value);
    }

    public bool IsDisconnectedState
    {
        get => _isDisconnectedState;
        private set => SetAndRaise(IsDisconnectedStateProperty, ref _isDisconnectedState, value);
    }

    public bool IsAccessDeniedState
    {
        get => _isAccessDeniedState;
        private set => SetAndRaise(IsAccessDeniedStateProperty, ref _isAccessDeniedState, value);
    }

    static AdminReady()
    {
        StateProperty.Changed.AddClassHandler<AdminReady>((x, _) => x.SyncStateFlags());
    }

    public AdminReady()
    {
        InitializeComponent();
        SyncStateFlags();
    }

    private void InitializeComponent()
    {
        AvaloniaXamlLoader.Load(this);
    }

    private void SyncStateFlags()
    {
        IsLoadingState = State == AdminReadyState.Loading;
        IsDisconnectedState = State == AdminReadyState.Disconnected;
        IsAccessDeniedState = State == AdminReadyState.AccessDenied;
    }
}
