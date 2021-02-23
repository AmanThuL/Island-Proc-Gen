using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using UnityEngine.SceneManagement;

public class WinLossManager : Singleton<WinLossManager>
{
    public GameObject WinLossCanvas;
    public GameObject youWinText;
    public GameObject youDieText;
    public GameObject sceneFader;


    public void GoToMenu()
    {
        Time.timeScale = 1;

        sceneFader.SetActive(true);
        sceneFader.GetComponent<SceneFader>().FadeTo("Menu");
    }

    public void Win()
    {
        Time.timeScale = 0;

        WinLossCanvas.SetActive(true);

        youWinText.SetActive(true);
        youDieText.SetActive(false);
    }

    public void Die()
    {
        Time.timeScale = 0;

        WinLossCanvas.SetActive(true);

        youDieText.SetActive(true);
        youWinText.SetActive(false);
    }
}
