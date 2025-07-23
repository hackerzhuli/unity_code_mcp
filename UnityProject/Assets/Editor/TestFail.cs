

using NUnit.Framework;
using UnityEngine;

namespace TestExecution.Editor
{
    public class TestFail{
        [Test]
        public void FailTest(){
            Debug.Log("FailTest");
            Assert.IsTrue(false);
        }
    }
}
