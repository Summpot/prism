using System;
using CommunityToolkit.Mvvm.ComponentModel;

namespace Prism.Services;

public partial class PanelSessionService : ObservableObject
{
    private static readonly Lazy<PanelSessionService> _instance = new(() => new PanelSessionService());
    public static PanelSessionService Instance => _instance.Value;

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsAuthenticated))]
    private string _baseUrl = "";

    [ObservableProperty]
    [NotifyPropertyChangedFor(nameof(IsAuthenticated))]
    private string _token = "";

    public bool IsAuthenticated => !string.IsNullOrWhiteSpace(Token);

    [ObservableProperty]
    private bool _isAdmin = false;

    [ObservableProperty]
    private string _username = "";

    [ObservableProperty]
    private string _displayName = "";

    [ObservableProperty]
    private string _avatarUrl = "";

    public event Action? SessionChanged;

    public void SetSession(string baseUrl, string token, bool isAdmin, string username, string displayName, string avatarUrl)
    {
        BaseUrl = baseUrl;
        Token = token;
        IsAdmin = isAdmin;
        Username = username;
        DisplayName = displayName;
        AvatarUrl = avatarUrl;
        SessionChanged?.Invoke();
    }

    public void SignIn(string token, string username = "User", bool isAdmin = false, string? baseUrl = null, string displayName = "", string avatarUrl = "")
    {
        SetSession(baseUrl ?? BaseUrl, token, isAdmin, username, string.IsNullOrEmpty(displayName) ? username : displayName, avatarUrl);
    }

    public void SignOut()
    {
        Token = "";
        IsAdmin = false;
        Username = "";
        DisplayName = "";
        AvatarUrl = "";
        SessionChanged?.Invoke();
    }
}
