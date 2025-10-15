document.addEventListener("DOMContentLoaded", function () {
  const terminalBody = document.getElementById("terminalBody");
  const commandInput = document.getElementById("commandInput");
  const navItems = document.querySelectorAll(".nav-item");
  const sections = document.querySelectorAll(".section");
  const demoOutput = document.getElementById("demo-output");

  // Available commands for auto-complete
  const availableCommands = [
    "help",
    "clear",
    "config",
    "keybindings",
    "keys",
    "bindings",
    "demo",
    "about",
    "date",
    "echo",
    "exit",
    "friends",
    "friendly",
    "projects",
  ];

  let commandHistory = [];
  let historyIndex = -1;
  let currentInput = "";

  // Navigation menu functionality
  navItems.forEach((item) => {
    item.addEventListener("click", function () {
      const sectionId = this.getAttribute("data-section") + "-section";

      // Update active nav item
      navItems.forEach((nav) => nav.classList.remove("active"));
      this.classList.add("active");

      // Show selected section
      sections.forEach((section) => {
        section.classList.add("hidden");
      });
      document.getElementById(sectionId).classList.remove("hidden");

      // Focus on input
      commandInput.focus();
    });
  });

  // Demo functionality
  document
    .getElementById("demo-feature1")
    .addEventListener("click", function () {
      const demoOutput = document.getElementById("demo-output");
      const videoPlaceholder = document.getElementById("video-placeholder");
      const allVideos = document.querySelectorAll(".demo-video");

      demoOutput.innerHTML = `
<span class="success">Basic Usage Demo</span><br>
Showing how to use Wallrs for basic wallpaper management...<br>
<br>
Commands demonstrated:<br>
- wallrs --path ~/Pictures/wallpapers<br>
<span class="success">✓ Basic functionality demonstrated</span>
    `;

      // Hide all videos and placeholder, show specific video
      videoPlaceholder.classList.add("hidden");
      allVideos.forEach((video) => {
        video.classList.add("hidden");
        video.pause(); // Pause all videos
      });

      // Show and play the specific video
      const demoVideo = document.getElementById("demo-video1");
      demoVideo.classList.remove("hidden");
      demoVideo.currentTime = 0; // Reset to start
      demoVideo.play().catch((e) => console.log("Autoplay prevented:", e));
    });
  document
    .getElementById("demo-feature2")
    .addEventListener("click", function () {
      const demoOutput = document.getElementById("demo-output");
      const videoPlaceholder = document.getElementById("video-placeholder");
      const allVideos = document.querySelectorAll(".demo-video");

      demoOutput.innerHTML = `
<span class="success">Renaming Wallpapers Demo</span><br>
Showing how to use Wallrs to rename your wallpapers...<br>
<br>
Commands demonstrated:<br>
- wallrs --path ~/Pictures/wallpapers<br>
<span class="success">✓ Renaming functionality demonstrated</span>
    `;

      // Hide all videos and placeholder, show specific video
      videoPlaceholder.classList.add("hidden");
      allVideos.forEach((video) => {
        video.classList.add("hidden");
        video.pause(); // Pause all videos
      });

      // Show and play the specific video
      const demoVideo = document.getElementById("demo-video2");
      demoVideo.classList.remove("hidden");
      demoVideo.currentTime = 0; // Reset to start
      demoVideo.play().catch((e) => console.log("Autoplay prevented:", e));
    });

  document.getElementById("demo-reset").addEventListener("click", function () {
    const demoOutput = document.getElementById("demo-output");
    const videoPlaceholder = document.getElementById("video-placeholder");
    const allVideos = document.querySelectorAll(".demo-video");

    demoOutput.textContent = "Select a demo above to see Wallrs in action!";

    // Hide all videos and show placeholder
    allVideos.forEach((video) => {
      video.classList.add("hidden");
      video.pause();
    });
    videoPlaceholder.classList.remove("hidden");
  });
  // Focus on input when clicking anywhere in the terminal
  terminalBody.addEventListener("click", function () {
    commandInput.focus();
  });

  // Handle command input
  commandInput.addEventListener("keydown", function (e) {
    if (e.key === "Enter") {
      const command = commandInput.value.trim();
      if (command) {
        // Add to command history
        commandHistory.unshift(command);
        historyIndex = -1;

        // Add the command to output
        addCommandOutput(command);

        // Process the command
        processCommand(command);

        // Clear input
        commandInput.value = "";

        // Scroll to bottom
        terminalBody.scrollTop = terminalBody.scrollHeight;
      }
    } else if (e.key === "Tab") {
      // Auto-complete on Tab
      e.preventDefault();
      autoComplete();
    } else if (e.key === "ArrowUp") {
      // Command history navigation
      e.preventDefault();
      navigateHistory("up");
    } else if (e.key === "ArrowDown") {
      // Command history navigation
      e.preventDefault();
      navigateHistory("down");
    }
  });

  function autoComplete() {
    const input = commandInput.value.trim();
    if (!input) return;

    // Find matching commands
    const matches = availableCommands.filter((cmd) =>
      cmd.startsWith(input.toLowerCase()),
    );

    if (matches.length === 1) {
      // Auto-complete with single match
      commandInput.value = matches[0] + " ";
    } else if (matches.length > 1) {
      // Show possible completions
      addTextOutput(`Possible completions: ${matches.join(", ")}`, "directory");
    }
  }

  function navigateHistory(direction) {
    if (commandHistory.length === 0) return;

    if (direction === "up") {
      if (historyIndex < commandHistory.length - 1) {
        if (historyIndex === -1) {
          currentInput = commandInput.value;
        }
        historyIndex++;
        commandInput.value = commandHistory[historyIndex];
      }
    } else if (direction === "down") {
      if (historyIndex > 0) {
        historyIndex--;
        commandInput.value = commandHistory[historyIndex];
      } else if (historyIndex === 0) {
        historyIndex = -1;
        commandInput.value = currentInput;
      }
    }
  }

  function addCommandOutput(command) {
    const outputDiv = document.createElement("div");
    outputDiv.className = "output";
    outputDiv.innerHTML = `<span class="prompt-user">user@docs</span><span class="prompt-symbol">:~$</span> <span class="command">${command}</span>`;
    terminalBody.insertBefore(outputDiv, commandInput.parentNode);
  }

  function addTextOutput(text, className = "") {
    const outputDiv = document.createElement("div");
    outputDiv.className = `output ${className}`;
    outputDiv.textContent = text;
    terminalBody.insertBefore(outputDiv, commandInput.parentNode);
  }

  function processCommand(command) {
    const cmd = command.toLowerCase();

    switch (cmd) {
      case "help":
        // Navigate to commands section
        navItems.forEach((nav) => nav.classList.remove("active"));
        document
          .querySelector('[data-section="commands"]')
          .classList.add("active");
        sections.forEach((section) => section.classList.add("hidden"));
        document.getElementById("commands-section").classList.remove("hidden");
        break;

      case "clear":
        // Remove all output elements except the last one (input line)
        const outputs = terminalBody.querySelectorAll(".output");
        outputs.forEach((output) => {
          if (
            output.parentNode === terminalBody &&
            output !== commandInput.parentNode
          ) {
            terminalBody.removeChild(output);
          }
        });
        break;

      case "config":
        // Navigate to config section
        navItems.forEach((nav) => nav.classList.remove("active"));
        document
          .querySelector('[data-section="config"]')
          .classList.add("active");
        sections.forEach((section) => section.classList.add("hidden"));
        document.getElementById("config-section").classList.remove("hidden");
        break;

      case "demo":
        // Navigate to demo section
        navItems.forEach((nav) => nav.classList.remove("active"));
        document.querySelector('[data-section="demo"]').classList.add("active");
        sections.forEach((section) => section.classList.add("hidden"));
        document.getElementById("demo-section").classList.remove("hidden");
        break;

      case "about":
        addTextOutput("Wallrs Documentation Terminal v0.1.7");
        addTextOutput(
          "A TUI app for changing the wallpaper on both x11 and wayland-.",
        );
        addTextOutput("🏳️‍⚧️ Built with Rust 🏳️‍⚧️ ");
        break;

      case "date":
        const now = new Date();
        addTextOutput(now.toString());
        break;

      case "exit":
        addTextOutput("This terminal cannot be closed from here.", "error");
        addTextOutput(
          "Use the close button in the title bar or close the browser tab.",
        );
        break;

      case "keybindings":
      case "keys":
      case "bindings":
        // Navigate to keybindings section
        navItems.forEach((nav) => nav.classList.remove("active"));
        document
          .querySelector('[data-section="keybindings"]')
          .classList.add("active");
        sections.forEach((section) => section.classList.add("hidden"));
        document
          .getElementById("keybindings-section")
          .classList.remove("hidden");
        break;

      case "projects":
      case "friendly":
      case "friends":
        // Navigate to friendly projects section
        navItems.forEach((nav) => nav.classList.remove("active"));
        document
          .querySelector('[data-section="friendly"]')
          .classList.add("active");
        sections.forEach((section) => section.classList.add("hidden"));
        document.getElementById("friendly-section").classList.remove("hidden");
        break;

      case "":
        // Do nothing for empty command
        break;

      default:
        if (cmd.startsWith("echo ")) {
          const text = command.substring(5);
          addTextOutput(text);
        } else {
          addTextOutput(`Command not found: ${command}`, "error");
          addTextOutput('Type "help" for available commands');
        }
        break;
    }
  }
});
