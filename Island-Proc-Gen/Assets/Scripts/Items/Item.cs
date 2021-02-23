using UnityEngine;

[CreateAssetMenu(fileName = "New Item", menuName = "Inventory/Item")]
public class Item : ScriptableObject
{
    new public string name = "New Item";
    public Sprite icon = null;
    public bool isDefaultItem = false;

    public virtual void Use()
    {
        // Use the item
        // Something might happen

        Debug.Log("Using " + name);

        bool isSuccessful = PlayerManager.Instance.UseItem(this);

        if (isSuccessful)
            Inventory.Instance.Remove(this);
        else
            Debug.Log("Please use the item within the theater's range.");
    }

    public void RemoveFromInventory()
    {
        Inventory.Instance.Remove(this);
    }
}
