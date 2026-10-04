using Avalonia.Media;

namespace Prism.Common;

public static class AppIcons
{
    private static StreamGeometry Parse(string data) => StreamGeometry.Parse(data);

    public static readonly StreamGeometry Gamepad2 = Parse("M6 11h4m-2-2v4m4 2l2-2m0 0l2 2m-2-2v4M2 6a4 4 0 0 1 4-4h12a4 4 0 0 1 4 4v12a4 4 0 0 1-4 4H6a4 4 0 0 1-4-4V6z");
    public static readonly StreamGeometry ArrowDownUp = Parse("M3 16l4 4 4-4M7 20V4m14 4l-4-4-4 4m4-4v16");
    public static readonly StreamGeometry Terminal = Parse("M4 17l6-6-6-6m8 14h8");
    public static readonly StreamGeometry SlidersHorizontal = Parse("M21 4h-7m-4 0H3m11-2v4m-7 6h14m-18 0h0m4-2v4m14 6h-4m-4 0H3m8-2v4");
    public static readonly StreamGeometry Sliders = Parse("M4 21v-7m0-4V3m8 18v-9m0-4V3m8 18v-5m0-4V3M1 14h6m2-6h6m2 8h6");
    public static readonly StreamGeometry Zap = Parse("M13 2L3 14h9l-1 8 10-12h-9l1-8z");
    public static readonly StreamGeometry Activity = Parse("M22 12h-4l-3 9L9 3l-3 9H2");
    public static readonly StreamGeometry Cable = Parse("M4 9a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V9zm10 0a2 2 0 0 1 2-2h2a2 2 0 0 1 2 2v2a2 2 0 0 1-2 2h-2a2 2 0 0 1-2-2V9zm-7 4v3a4 4 0 0 0 4 4h2a4 4 0 0 0 4-4v-3");
    public static readonly StreamGeometry Unplug = Parse("M19 5l3 3-4.5 4.5M2 22l8-8m-3-1l4-4m-1 7l4-4m1.5-3.5L14 3 9.5 7.5l7 7L21 10z");
    public static readonly StreamGeometry Server = Parse("M2 4a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4zm0 12a2 2 0 0 1 2-2h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-4zm4-9h.01M6 19h.01");
    public static readonly StreamGeometry Radio = Parse("M4.9 19.1C1 15.2 1 8.8 4.9 4.9m14.2 0c3.9 3.9 3.9 10.3 0 14.2M7.8 16.2c-2.3-2.3-2.3-6.1 0-8.5m8.4 0c2.3 2.3 2.3 6.1 0 8.5M12 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2z");
    public static readonly StreamGeometry Gauge = Parse("M12 2a10 10 0 0 0-7.07 17.07l2.83-2.83A6 6 0 1 1 12 18v3a10 10 0 0 0 0-20zm0 6a4 4 0 0 0-4 4c0 .74.2 1.43.55 2.03L12 12l2.45-3.97A3.98 3.98 0 0 0 12 8z");
    public static readonly StreamGeometry Users = Parse("M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2m18 0v-2a4 4 0 0 0-3-3.87m-4-12a4 4 0 0 1 0 7.75M9 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0z");
    public static readonly StreamGeometry Settings = Parse("M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2zM12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z");
    public static readonly StreamGeometry Shield = Parse("M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z");
    public static readonly StreamGeometry Plug = Parse("M12 2v6m4-6v6M9 8h6a3 3 0 0 1 3 3v2a6 6 0 0 1-6 6v3m-3-3a6 6 0 0 1-6-6v-2a3 3 0 0 1 3-3h6");
    public static readonly StreamGeometry Power = Parse("M18.36 6.64a9 9 0 1 1-12.73 0M12 2v10");
    public static readonly StreamGeometry Check = Parse("M20 6L9 17l-5-5");
    public static readonly StreamGeometry ChevronDown = Parse("M6 9l6 6 6-6");
    public static readonly StreamGeometry Copy = Parse("M10 8h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H10a2 2 0 0 1-2-2V10a2 2 0 0 1 2-2zM4 16a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2");
    public static readonly StreamGeometry Plus = Parse("M12 5v14m-7-7h14");
    public static readonly StreamGeometry Download = Parse("M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4m4-5l5 5 5-5m-5 5V3");
    public static readonly StreamGeometry Share2 = Parse("M18 8a3 3 0 1 0 0-6 3 3 0 0 0 0 6zM6 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6zm12 7a3 3 0 1 0 0-6 3 3 0 0 0 0 6zm-9.41-5.09l6.83 3.98m-.01-11.78l-6.82 3.98");
    public static readonly StreamGeometry Trash2 = Parse("M3 6h18m-2 0v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2m-6 5v6m4-6v6");
    public static readonly StreamGeometry RotateCcw = Parse("M3 12a9 9 0 1 0 2.8-6.4L3 8m0 0V3m0 5h5");
    public static readonly StreamGeometry WifiOff = Parse("M1 1l22 22M16.72 11.06A10.94 10.94 0 0 1 19 12.55M5 12.55a10.94 10.94 0 0 1 5.17-2.39M10.71 5.05A16 16 0 0 1 22.58 9M1.42 9a15.91 15.91 0 0 1 4.7-2.88m3.43 9.43a4 4 0 0 1 4.9 0M12 20h.01");
    public static readonly StreamGeometry Search = Parse("M21 21l-6-6m2-5a7 7 0 1 1-14 0 7 7 0 0 1 14 0z");
    public static readonly StreamGeometry ArrowDown = Parse("M12 5v14m7-7l-7 7-7-7");
    public static readonly StreamGeometry Cpu = Parse("M4 4h16v16H4V4zm5-4v4m6-4v4m-6 16v4m6-4v4M0 9h4m-4 6h4m16-6h4m-4 6h4m-11-6h6v6H9V9z");
    public static readonly StreamGeometry Close = Parse("M18 6L6 18M6 6l12 12");
    public static readonly StreamGeometry PanelLeft = Parse("M3 3h18v18H3V3zm6 0v18");
    public static readonly StreamGeometry Github = Parse("M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.53 1.032 1.53 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0 1 12 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0 0 22 12.017C22 6.484 17.522 2 12 2z");
    public static readonly StreamGeometry WindowMinimize = Parse("M2 6h8");
    public static readonly StreamGeometry WindowMaximize = Parse("M1.5 1.5h9v9h-9z");
    public static readonly StreamGeometry WindowRestore = Parse("M3.5 2.5V1.5h7v7H9.5M.5 3.5h7v7h-7z");
    public static readonly StreamGeometry WindowClose = Parse("M2.5 2.5l7 7m0-7l-7 7");
    public static readonly StreamGeometry Languages = Parse("M5 8l6 6M4 14l6-6 2-3M2 5h12M7 2h1M22 22l-5-10-5 10M14 18h6");
    public static readonly StreamGeometry Palette = Parse("M12 2C6.5 2 2 6.5 2 12c0 3.6 2 6.8 5 8.2.5.2 1.1-.1 1.1-.7v-1.1c0-.5.4-1 1-1h1.5c3.6 0 6.5-2.9 6.5-6.4 0-4.4-4-8-9.1-8z");
    public static readonly StreamGeometry ArrowUpCircle = Parse("M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zm0-6V8m-4 4l4-4 4 4");
    public static readonly StreamGeometry Info = Parse("M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20zm0-8v4m0-8h.01");
    public static readonly StreamGeometry AlertTriangle = Parse("M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0zM12 9v4m0 4h.01");
    public static readonly StreamGeometry Monitor = Parse("M4 3h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zm8 14v4m-4 0h8");
    public static readonly StreamGeometry Sun = Parse("M12 1v2m0 18v2M4.22 4.22l1.42 1.42m12.72 12.72l1.42 1.42M1 12h2m18 0h2M4.22 19.78l1.42-1.42m12.72-12.72l1.42-1.42M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10z");
    public static readonly StreamGeometry Moon = Parse("M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9z");
    public static readonly StreamGeometry ExternalLink = Parse("M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6m4-3h6v6m-11 5L21 3");
    public static readonly StreamGeometry ArrowDownToLine = Parse("M12 17V3m-6 8l6 6 6-6M19 21H5");
    public static readonly StreamGeometry WrapText = Parse("M3 6h18M3 12h15a3 3 0 1 1 0 6h-4m2-2l-2 2 2 2M3 18h7");
}
