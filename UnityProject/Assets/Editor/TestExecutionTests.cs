using NUnit.Framework;
using UnityEngine;
using UnityEditor;

namespace TestExecution.Editor
{
    /// <summary>
    /// Simple EditMode tests for testing the Unity Code MCP test execution functionality
    /// </summary>
    public class TestExecutionTests
    {
        [Test]
        public void SimplePassingTest()
        {
            // A simple test that always passes
            Assert.IsTrue(true, "This test should always pass");
            Debug.Log("SimplePassingTest executed successfully");
        }

        [Test]
        public void MathTest()
        {
            // Test basic math operations
            int result = 2 + 2;
            Assert.AreEqual(4, result, "2 + 2 should equal 4");
            Debug.Log("MathTest completed with result: " + result);
        }

        [Test]
        public void StringTest()
        {
            // Test string operations
            string hello = "Hello";
            string world = "World";
            string combined = hello + " " + world;
            
            Assert.AreEqual("Hello World", combined, "String concatenation should work correctly");
            Debug.Log("StringTest completed with result: " + combined);
        }

        [Test]
        public void UnityObjectTest()
        {
            // Test Unity-specific functionality
            GameObject testObject = new GameObject("TestObject");
            Assert.IsNotNull(testObject, "GameObject should be created successfully");
            Assert.AreEqual("TestObject", testObject.name, "GameObject name should be set correctly");
            
            // Clean up
            Object.DestroyImmediate(testObject);
            Debug.Log("UnityObjectTest completed successfully");
        }

        [Test]
        public void SlowTest()
        {
            // A test that takes some time to complete
            Debug.Log("SlowTest starting...");
            
            // Simulate some work
            System.Threading.Thread.Sleep(100);
            
            Assert.IsTrue(true, "Slow test should complete");
            Debug.Log("SlowTest completed after delay");
        }
    }
}