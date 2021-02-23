using System.Collections;
using System.Collections.Generic;
using UnityEngine;

public enum CursorType
{
    Basic,
    Weapon_Axe,
    Weapon_Hammer,
    Weapon,
    Pickup
}

public class CursorManager : Singleton<CursorManager>
{
    public CursorTypeTextureDictionary cursorTypeTextureDict = new CursorTypeTextureDictionary();

    // Start is called before the first frame update
    void Start()
    {
        ResetCursor();
    }

    public void SetCursor(CursorType cursorType, Vector2 hotspot)
    {
        Cursor.SetCursor(cursorTypeTextureDict[cursorType], hotspot, CursorMode.ForceSoftware);
    }

    public void ResetCursor()
    {
        SetCursor(CursorType.Basic, Vector2.zero);
    }
}
