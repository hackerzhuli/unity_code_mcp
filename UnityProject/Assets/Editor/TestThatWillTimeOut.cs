using NUnit.Framework;
using UnityEngine;
using UnityEditor;
using UnityEngine.TestTools;
using System.Collections;
using System.Threading;

namespace TestExecution.Editor
{
    public class TestThatWillTimeOut
    {
        [Test]
        public void NormalTest(){
            Debug.Log("NormalTest");
        }

        [Test]
        public void LongTest()
        {
            // a test that will timeout our tool call(for debug builds, but not for release builds)
            Debug.Log("LongTest");
            //Thread.Sleep(35000);
        }
    }
}
