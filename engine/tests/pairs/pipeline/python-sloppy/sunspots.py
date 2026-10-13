import os

import matplotlib.pyplot as plt
import pandas as pd
import requests

df = None


def get_data():
    global df
    url = "https://www.sidc.be/SILSO/DATA/SN_m_tot_V2.0.csv"
    r = requests.get(url)
    os.makedirs("data/raw", exist_ok=True)
    with open("data/raw/sunspots.csv", "w") as f:
        f.write(r.text)
    df = pd.read_csv("data/raw/sunspots.csv", sep=";", header=None)
    df.columns = ["year", "month", "decimal_date", "mean", "std", "obs", "definitive"]


def clean():
    df["date"] = pd.to_datetime(df["year"].astype(str) + "-" + df["month"].astype(str))
    df.drop(df[df["mean"] == -1].index, inplace=True)
    df.reset_index(drop=True, inplace=True)


def smooth():
    df["smooth"] = df["mean"].rolling(13, center=True).mean()


def plot(**kwargs):
    plt.figure(figsize=(12, 5))
    plt.plot(df["date"], df["mean"], label="monthly mean", **kwargs)
    plt.plot(df["date"], df["smooth"], label="13 month mean", color="red")
    plt.title("Sunspots")
    plt.legend()
    os.makedirs("figures", exist_ok=True)
    plt.savefig("figures/sunspots.png")


if __name__ == "__main__":
    get_data()
    clean()
    smooth()
    plot(color="grey", linewidth=0.5)
