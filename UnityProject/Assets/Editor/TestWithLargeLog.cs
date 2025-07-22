namespace TestExecution.Editor
{
    public class TestWithLargeLog 
    {
        [Test]
        public void LargeMessageTest()
        {
            // Test that generates a large log message to test TCP fallback functionality
            Debug.Log("LargeMessageTest starting - generating large log message...");
            
            // Generate a large message (over 8KB) to test TCP fallback
            System.Text.StringBuilder sb = new System.Text.StringBuilder();
            sb.AppendLine("=== LARGE MESSAGE TEST FROM EDIT MODE TEST ===");
            
            // Add enough content to exceed 8KB
            for (int i = 0; i < 50; i++)
            {
                sb.AppendLine($"Line {i:D3}: This is a test line with some content to make the message large. Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.");
            }
            
            sb.AppendLine("=== END LARGE MESSAGE TEST ===");
            
            string largeMessage = sb.ToString();
            
            // Log the large message - this should trigger TCP fallback if message > 8KB
            Debug.Log($"Generated large message with {largeMessage.Length} characters:\n{largeMessage}");
            
            // Verify the test passes
            Assert.IsTrue(largeMessage.Length > 8000, "Large message should be over 8KB to test TCP fallback");
            Debug.Log("LargeMessageTest completed successfully - TCP fallback should have been triggered");
        }
    }
}