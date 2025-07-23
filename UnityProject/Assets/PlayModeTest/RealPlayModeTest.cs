using System.Collections;
using NUnit.Framework;
using UnityEngine;
using UnityEngine.TestTools;

namespace A{
    
}
public class RealPlayModeTest
{
    [Test]
    public void RealPlayModeTestSimplePasses()
    {
        Debug.Log("Test");
        // Use the Assert class to test conditions
        Assert.AreEqual(2, 2);

        
        
    }

    [UnityTest]
    public IEnumerator RealPlayModeTestWithEnumeratorPasses()
    {
        yield return new WaitForSecondsRealtime(1);
        // Use the Assert class to test conditions.
        // Use yield to skip a frame.
        yield return null;
    }

    [UnityTest]
    public IEnumerator RealPlayModeTestWithEnumeratorPasses2()
    {
        yield return new WaitForSecondsRealtime(1);
        // Use the Assert class to test conditions.
        // Use yield to skip a frame.
        yield return null;
    }

    [UnityTest]
    public IEnumerator RealPlayModeTestWithEnumeratorPasses3()
    {
        yield return new WaitForSecondsRealtime(1);
        // Use the Assert class to test conditions.
        // Use yield to skip a frame.
        yield return null;
    }
}
