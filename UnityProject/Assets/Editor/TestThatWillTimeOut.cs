using NUnit.Framework;
using UnityEngine;
using UnityEditor;
using UnityEngine.TestTools;
using System.Collections;

namespace TestExecution.Editor
{
    public class TestThatWillTimeOut
    {
        [Test]
        public void NormalTest(){
            Debug.Log("NormalTest");
        }

        [UnityTest]
        public IEnumerator LongTest()
        {
            // a test that will timeout our tool call(for debug builds, but not for release builds)
            Debug.Log("LongTest");
            yield return new WaitForSeconds(10);
        }
    }
}
