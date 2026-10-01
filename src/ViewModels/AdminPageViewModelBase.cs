using System;
using System.Threading;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.I18n;
using Prism.Services;

namespace Prism.ViewModels;

public abstract partial class AdminPageViewModelBase : ViewModelBase, INavigationAware
{
    protected readonly AdminApiClient Api = AdminApiClient.Instance;
    protected readonly NativeClientService Client = NativeClientService.Instance;

    [ObservableProperty]
    private AdminReadyState _readyState = AdminReadyState.Loading;

    [ObservableProperty]
    private bool _isReady;

    [ObservableProperty]
    private bool _isLoading;

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private bool _autoRefresh = true;

    [ObservableProperty]
    private string _autoRefreshText = "";

    private CancellationTokenSource? _autoRefreshCts;

    protected abstract TimeSpan RefreshInterval { get; }

    protected virtual bool PauseAutoRefresh => false;

    protected AdminPageViewModelBase()
    {
        UpdateAutoRefreshText();
    }

    public virtual void OnNavigatedTo()
    {
        UpdateAutoRefreshText();
        _ = RefreshAsync();
        if (AutoRefresh)
        {
            StartAutoRefresh();
        }
    }

    public virtual void OnNavigatedFrom()
    {
        StopAutoRefresh();
    }

    partial void OnAutoRefreshChanged(bool value)
    {
        UpdateAutoRefreshText();
        if (value) StartAutoRefresh();
        else StopAutoRefresh();
    }

    protected void UpdateAutoRefreshText()
    {
        string stateStr = AutoRefresh
            ? I18nText.T("admin_on", "on")
            : I18nText.T("admin_off", "off");
        AutoRefreshText = I18nText.T("admin_auto_refresh", "Auto-refresh {state}").Replace("{state}", stateStr);
    }

    [RelayCommand]
    public void ToggleAutoRefresh() => AutoRefresh = !AutoRefresh;

    [RelayCommand]
    public void GoToConnect()
    {
        NavigationService.Instance.NavigateTo("client.overview");
    }

    [RelayCommand]
    public void DismissError()
    {
        ErrorMessage = null;
    }

    protected void StartAutoRefresh()
    {
        AutoRefreshHelper.Stop(ref _autoRefreshCts);
        if (AutoRefresh)
        {
            _autoRefreshCts = AutoRefreshHelper.Start(
                RefreshAsync,
                RefreshInterval,
                () => IsReady && !IsLoading && !PauseAutoRefresh);
        }
    }

    protected void StopAutoRefresh() => AutoRefreshHelper.Stop(ref _autoRefreshCts);

    protected bool BeginReadyCheck()
    {
        if (Client.CurrentStatus?.Running != true)
        {
            ReadyState = AdminReadyState.Disconnected;
            IsReady = false;
            ErrorMessage = null;
            return false;
        }

        var session = Api.CurrentSession;
        if (session != null && session.Authenticated && !session.IsAdmin)
        {
            ReadyState = AdminReadyState.AccessDenied;
            IsReady = false;
            ErrorMessage = null;
            return false;
        }

        ReadyState = AdminReadyState.Ready;
        IsReady = true;
        return true;
    }

    [RelayCommand]
    public async Task RefreshAsync()
    {
        if (!BeginReadyCheck())
        {
            await ClearWhenNotReadyAsync();
            return;
        }

        try
        {
            IsLoading = true;
            ErrorMessage = null;
            await RefreshCoreAsync();
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
        }
        finally
        {
            IsLoading = false;
        }
    }

    protected virtual Task ClearWhenNotReadyAsync() => Task.CompletedTask;

    protected abstract Task RefreshCoreAsync();
}
