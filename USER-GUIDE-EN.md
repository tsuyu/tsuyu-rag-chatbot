# USER GUIDE — TSUYU RAG Chatbot

A quick guide for **TSUYU users** using this chatbot to find information in official
documents. No technical knowledge required.

---

## What is this chatbot?

It is a question-answering assistant that answers **based solely on official TSUYU documents**
(policies, SOPs, circulars, etc.) that have been uploaded into the system. You ask in
**Bahasa Malaysia**, and it will:

1. Find the most relevant excerpts from TSUYU documents,
2. Compose an answer based on those excerpts,
3. List the **source documents** so you can verify yourself.

It is **not** like a general internet chatbot — it does not answer from "general knowledge"
or the internet. If the answer is not in TSUYU documents, it will say so.

---

## How to ask effectively

| Good practice | Example |
|---|---|
| **Be specific** | "How many days annual leave for a Grade 41 officer?" — better than "leave?" |
| **One topic per question** | Ask one thing, then ask follow-ups |
| **Use terms in documents** | If documents say "overtime allowance", use that term |
| **Follow-up questions allowed** | "What about Grade 44?" — it remembers conversation context |

**Avoid:**
- Too general questions ("tell me about TSUYU")
- Several different questions in one sentence
- Assuming it knows information outside documents (weather, news, personal matters)

---

## Reading answers

Every answer is accompanied by a **list of sources** — documents and (for PDFs) the page
number where the information was taken from.

> ✅ **Always check sources.** For important decisions, open the listed original document
> and verify the information yourself. The chatbot helps you *find* information quickly —
> it does not replace official documents.

### Saving answers

- **Copy** — click the 📋 button below any answer to copy its text.
- **Print** — click 🖨️ to print the entire conversation (or save as PDF via
  the browser's print dialog).
- **Export** — click ⬇️ to download the conversation as a `.md` or `.txt` file
  (containing questions, answers, and reference list).

All of this happens in your browser only — no data is sent anywhere.

---

## When chatbot says "information not found"

You may see a message like:

> *"Sorry, I could not find information related to this question in TSUYU documents…"*

This is **not a malfunction** — it is a safety feature (*guardrail*) that prevents the
chatbot from "making up" answers when it cannot find sufficiently relevant information.
It is better for it to say "I don't know" than to give a wrong answer.

**What you can do:**
- **Try again with different words** — use terms that might be used in the documents.
- **Be more specific** or more general.
- If you are confident the relevant document should be there, **notify an administrator** —
  the document may not have been uploaded yet.

---

## Limits you should know

- **Only knows what is in uploaded documents.** New documents must be uploaded by an
  administrator first.
- **Can make mistakes.** Even with guardrails, AI sometimes misinterprets.
  Sources are provided so you can verify — use them.
- **Not official advice.** Answers are assistance in finding information, not official
  decisions or legal advice.
- **Conversations may be saved** to enable follow-up questions. Avoid entering
  unnecessary sensitive personal information.

---

## Frequently asked questions (FAQ)

**Can I ask in English?**
The system is optimized for Bahasa Malaysia. BM questions give the best results.

**Why do answers for the same question sometimes differ slightly?**
AI regenerates sentences each time. The meaning should be consistent; if significantly
different, check the sources.

**Is my data sent outside?**
No. The entire system runs on TSUYU's server — no data goes to the internet.

**I found a wrong answer. What should I do?**
Check the listed source documents, and notify the system administrator so documents
can be updated if needed.

---

> For administrators & IT staff, see [README-EN.md](README-EN.md), [RUNBOOK-EN.md](RUNBOOK-EN.md),
> and [DOCUMENT-GUIDE-EN.md](DOCUMENT-GUIDE-EN.md).
