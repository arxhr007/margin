using System.Runtime.InteropServices;
using Margin.Touchpad;

namespace Margin.Input;

/// Low-level mouse hook: swallows edge clicks, their drags and releases, so apps never see them.
/// The callback must stay fast — Windows silently drops hooks that take too long — so all real work
/// (key presses, brightness, HUD) is posted to the UI queue by the engine.
internal sealed class MouseHook : IDisposable
{
    private readonly Native.LowLevelProc proc; // keep the delegate alive
    private readonly RawTouchpad touchpad;
    private readonly GestureEngine engine;
    private IntPtr hook;
    private bool draining;
    private int swallowedButton; // WM_LBUTTONUP / WM_RBUTTONUP we owe a swallow, 0 if none

    public MouseHook(RawTouchpad touchpad, GestureEngine engine)
    {
        this.touchpad = touchpad;
        this.engine = engine;
        proc = Callback;
        Install();
    }

    public void Install()
    {
        if (hook != IntPtr.Zero) Native.UnhookWindowsHookEx(hook);
        hook = Native.SetWindowsHookEx(Native.WH_MOUSE_LL, proc, Native.GetModuleHandle(null), 0);
        if (hook == IntPtr.Zero) Log.Error($"SetWindowsHookEx failed: {Marshal.GetLastWin32Error()}");
    }

    private IntPtr Callback(int code, IntPtr wParam, IntPtr lParam)
    {
        if (code >= 0 && !draining)
        {
            var info = Marshal.PtrToStructure<Native.MSLLHOOKSTRUCT>(lParam);
            if ((info.Flags & Native.LLMHF_INJECTED) == 0 && Handle((int)wParam))
                return new IntPtr(1);
        }
        return Native.CallNextHookEx(hook, code, wParam, lParam);
    }

    private bool Handle(int msg)
    {
        switch (msg)
        {
            case Native.WM_MOUSEMOVE:
                return engine.FreezePointer;

            case Native.WM_LBUTTONDOWN:
            case Native.WM_RBUTTONDOWN:
                // Raw input for this click may still be queued behind the hook call; read it first.
                draining = true;
                try { touchpad.Drain(); }
                finally { draining = false; }

                if (engine.MouseDown())
                {
                    swallowedButton = msg == Native.WM_LBUTTONDOWN ? Native.WM_LBUTTONUP : Native.WM_RBUTTONUP;
                    Log.Debug($"swallow {(msg == Native.WM_LBUTTONDOWN ? "left" : "right")} click");
                    return true;
                }
                return false;

            case Native.WM_LBUTTONUP:
            case Native.WM_RBUTTONUP:
                if (msg != swallowedButton) return false;
                swallowedButton = 0;
                engine.MouseUp();
                return true;
        }
        return false;
    }

    public void Dispose()
    {
        if (hook != IntPtr.Zero) Native.UnhookWindowsHookEx(hook);
        hook = IntPtr.Zero;
    }
}
