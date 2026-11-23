const form = document.getElementById("form");
const urlField = document.getElementById("url");
const errorBox = document.getElementById("error");
const result = document.getElementById("result");
const link = document.getElementById("link");
const refreshButton = document.getElementById("refresh");
const total = document.getElementById("total");
const days = document.getElementById("days");
let currentCode = null;

function cell(text) {
  const td = document.createElement("td");
  td.textContent = text;
  return td;
}

async function refreshStats() {
  if (!currentCode) return;
  const resp = await fetch(`/api/links/${currentCode}/stats`);
  if (!resp.ok) {
    errorBox.textContent = "could not load the stats";
    return;
  }
  const data = await resp.json();
  total.textContent = String(data.total);
  days.replaceChildren(
    ...data.by_day.map((d) => {
      const tr = document.createElement("tr");
      tr.append(cell(d.day), cell(String(d.clicks)));
      return tr;
    })
  );
}

form.addEventListener("submit", async (ev) => {
  ev.preventDefault();
  errorBox.textContent = "";
  const resp = await fetch("/api/links", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ url: urlField.value }),
  });
  const data = await resp.json();
  if (!resp.ok) {
    errorBox.textContent = data.error || "could not shorten the link";
    return;
  }
  currentCode = data.code;
  link.textContent = `${location.origin}/${currentCode}`;
  link.href = `/${currentCode}`;
  result.hidden = false;
  await refreshStats();
});

refreshButton.addEventListener("click", refreshStats);
