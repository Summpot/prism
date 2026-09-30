using System;
using System.Threading.Tasks;
using CommunityToolkit.Mvvm.ComponentModel;
using CommunityToolkit.Mvvm.Input;
using Prism.Services;

namespace Prism.ViewModels;

public partial class LoginViewModel : ViewModelBase
{
    private readonly PanelSessionService _session = PanelSessionService.Instance;
    private readonly NavigationService _nav = NavigationService.Instance;
    private readonly NativeClientService _client = NativeClientService.Instance;

    [ObservableProperty]
    private string _baseUrl = "http://127.0.0.1:8080";

    [ObservableProperty]
    private string _staticToken = "";

    [ObservableProperty]
    private string? _errorMessage;

    [ObservableProperty]
    private bool _isConnecting;

    [RelayCommand]
    public async Task ConnectWithTokenAsync()
    {
        if (string.IsNullOrWhiteSpace(BaseUrl)) return;
        try
        {
            IsConnecting = true;
            ErrorMessage = null;

            string normalizedUrl = BaseUrl.Trim().TrimEnd('/');
            string token = StaticToken.Trim();

            _session.SetSession(normalizedUrl, token, true, "admin", "Administrator", "");

            try
            {
                var health = await AdminApiClient.Instance.GetHealthAsync();
                if (!health.Ok)
                {
                    throw new Exception("Server reported unhealthy state.");
                }
            }
            catch (Exception ex)
            {
                _session.SignOut();
                throw new Exception($"Verification failed: {ex.Message}");
            }

            _nav.NavigateTo("admin.overview");
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
        }
        finally
        {
            IsConnecting = false;
        }
    }

    [RelayCommand]
    public void LoginWithGitHub()
    {
        try
        {
            string url = $"{BaseUrl.TrimEnd('/')}/auth/github?redirect_uri=prism://auth/callback";
            _client.OpenExternalUrl(url);
        }
        catch (Exception ex)
        {
            ErrorMessage = ex.Message;
        }
    }
}
