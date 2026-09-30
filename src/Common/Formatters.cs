using System;

namespace Prism.Common;

public static class Formatters
{
    private static readonly string[] ByteUnits = { "B", "KB", "MB", "GB", "TB", "PB" };
    private static readonly string[] BitUnits = { "bps", "Kbps", "Mbps", "Gbps", "Tbps" };

    public static string FormatBytes(ulong bytes)
    {
        if (bytes == 0) return "0 B";
        double len = bytes;
        int order = 0;
        while (len >= 1024 && order < ByteUnits.Length - 1)
        {
            order++;
            len /= 1024;
        }
        return $"{len:0.##} {ByteUnits[order]}";
    }

    public static string FormatPercentage(double ratio)
    {
        return $"{(ratio * 100):0.#}%";
    }

    public static string FormatUptime(ulong totalSeconds)
    {
        var ts = TimeSpan.FromSeconds(totalSeconds);
        if (ts.TotalDays >= 1)
        {
            return $"{(int)ts.TotalDays}d {ts.Hours}h {ts.Minutes}m";
        }
        if (ts.TotalHours >= 1)
        {
            return $"{ts.Hours}h {ts.Minutes}m {ts.Seconds}s";
        }
        if (ts.TotalMinutes >= 1)
        {
            return $"{ts.Minutes}m {ts.Seconds}s";
        }
        return $"{ts.Seconds}s";
    }

    public static string FormatBitsPerSecond(double bps)
    {
        if (bps <= 0) return "0 bps";
        double val = bps;
        int order = 0;
        while (val >= 1000 && order < BitUnits.Length - 1)
        {
            order++;
            val /= 1000;
        }
        return $"{val:0.##} {BitUnits[order]}";
    }

    public static string FormatBitRate(double bps) => FormatBitsPerSecond(bps);
}
