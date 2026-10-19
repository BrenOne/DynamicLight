# Inspect the real HWND icons of the isolated smoke-test process, not the HTML logo.
param([Parameter(Mandatory = $true)][int]$ProcessId)
$ErrorActionPreference = 'Stop'
Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Runtime.InteropServices;
public static class DynamicLightIconCheck {
  private delegate bool EnumProc(IntPtr hwnd, IntPtr data);
  [DllImport("user32.dll")] private static extern bool EnumWindows(EnumProc callback, IntPtr data);
  [DllImport("user32.dll")] private static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
  [DllImport("user32.dll")] private static extern bool ShowWindow(IntPtr hwnd, int command);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] private static extern int GetWindowText(IntPtr hwnd, System.Text.StringBuilder text, int count);
  [DllImport("user32.dll")] private static extern IntPtr SendMessageTimeout(IntPtr hwnd, uint msg, IntPtr wparam, IntPtr lparam, uint flags, uint timeout, out IntPtr result);
  public static void Check(int pid, string directory) {
    int windows = 0;
    EnumWindows(delegate(IntPtr hwnd, IntPtr data) {
      uint owner; GetWindowThreadProcessId(hwnd, out owner);
      if (owner != pid) return true;
      var title = new System.Text.StringBuilder(128);
      GetWindowText(hwnd, title, title.Capacity);
      if (title.ToString() != "DynamicLight" && !(title.ToString().StartsWith("DynamicLight") && title.ToString().Contains("Quick controls"))) return true;
      if (title.ToString() == "DynamicLight") ShowWindow(hwnd, 3);
      foreach (int kind in new[] {0, 1}) {
        IntPtr handle;
        if (SendMessageTimeout(hwnd, 0x7f, (IntPtr)kind, IntPtr.Zero, 2, 1000, out handle) == IntPtr.Zero || handle == IntPtr.Zero)
          throw new Exception("Native window icon is missing: " + title + " / " + kind);
        // The window owns the HICON. Dispose only the managed wrapper/bitmap.
        using (var icon = Icon.FromHandle(handle))
        using (var bitmap = icon.ToBitmap()) {
          int purple = 0, mint = 0;
          for (int y = 0; y < bitmap.Height; y++) for (int x = 0; x < bitmap.Width; x++) {
            var c = bitmap.GetPixel(x, y);
            if (c.A < 128) continue;
            if (c.R > 90 && c.B > 140 && c.B > c.G * 1.1) purple++;
            if (c.G > 140 && c.G > c.R * 1.25 && c.B > 90) mint++;
          }
          if (purple < bitmap.Width * bitmap.Height / 10 || mint > bitmap.Width * bitmap.Height / 5)
            throw new Exception("Native window still has the old/missing D artwork: " + title + " / " + kind);
          bitmap.Save(Path.Combine(directory, "native-" + (title.ToString() == "DynamicLight" ? "main" : "quick") + "-icon-" + kind + ".png"), ImageFormat.Png);
        }
      }
      windows++;
      return true;
    }, IntPtr.Zero);
    if (windows < 2) throw new Exception("Expected main and Quick controls windows for native icon checks");
    Console.WriteLine("Main and Quick controls title-bar/taskbar icons use the D artwork");
  }
}
'@
[DynamicLightIconCheck]::Check($ProcessId, (Join-Path (Get-Location) '.local'))
