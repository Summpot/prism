using System;
using CommunityToolkit.Mvvm.ComponentModel;

namespace Prism.Services;

public class NavigationService : ObservableObject
{
    private static readonly Lazy<NavigationService> _instance = new(() => new NavigationService());
    public static NavigationService Instance => _instance.Value;

    private string _currentRoute = "client.overview";
    public string CurrentRoute
    {
        get => _currentRoute;
        set
        {
            if (SetProperty(ref _currentRoute, value))
            {
                Navigated?.Invoke(value);
            }
        }
    }

    private ObservableObject? _currentViewModel;
    public ObservableObject? CurrentViewModel
    {
        get => _currentViewModel;
        set => SetProperty(ref _currentViewModel, value);
    }

    public event Action<string>? Navigated;

    public void NavigateTo(string route)
    {
        CurrentRoute = route;
    }
}
