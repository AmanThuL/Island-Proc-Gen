using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.SceneManagement;

public class PlayerManager : Singleton<PlayerManager>
{
    public GameObject player;
    public TheaterController theaterController;

    private void Start()
    {
        //theaterController = GameObject.Find("Theater(Clone)").GetComponent<TheaterController>();
    }

    private void Update()
    {
        if (theaterController == null)
        {
            theaterController = GameObject.Find("Theater(Clone)").GetComponent<TheaterController>();
        }
        if (Input.GetKeyDown(KeyCode.Escape))
        {
            Application.Quit();
        }
    }

    public void KillPlayer()
    {
        Debug.Log("Current active scene index is " + SceneManager.GetActiveScene().buildIndex);
        SceneManager.LoadScene(SceneManager.GetActiveScene().buildIndex);
    }

    public bool UseItem(Item item)
    {
        if (item.name == "Watermelon" || item.name == "Starbucks" || item.name == "Sofa")
        {
            if (theaterController.IsWithinActivationRange(player.transform))
            {
                theaterController.ActivateItem(item.name);
                return true;
            }
        }
        return false;
    }
}
