using System;

namespace Prism.Services;

public class PanelSessionService
{
    private static readonly Lazy<PanelSessionService> _instance = new(() => new PanelSessionService());
    public static PanelSessionService Instance => _instance.Value;

    public string BaseUrl { get; set; } = "http://127.0.0.1:8080";
    public string Token { get; set; } = "";
    public bool IsAuthenticated => !string.IsNullOrWhiteSpace(Token);
    public bool IsAdmin { get; set; } = false;
    public string Username { get; set; } = "";
    public string DisplayName { get; set; } = "";
    public string AvatarUrl { get; set; } = "";

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
