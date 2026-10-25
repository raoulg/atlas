---
atlas-script: demo.py
---
# What does a vector cache give us?

A frozen encoder can turn each flower image into a vector once. The small script here starts with three saved vectors and classifies two new vectors by cosine similarity.

## Follow one prediction

`Tensor1D` names the shape of a query vector. `Tensor2D` names the cache: one row per flower. The function `nearest` compares the query with each cached row and returns the nearest label.

<atlas-view ref="views/prediction.json" at="_root.demo.nearest" title="From cached vectors to a label" source="demo.py" />

Run the ordinary Python script to see both predictions in the terminal. Open the full Atlas view to follow the entire route.

## The picture from this run

The script writes this figure to the lesson's `output/` directory. The Markdown refers to the file by name; no annotation in the Python code is needed.

![Three cached flower vectors](output/flowers.svg)
