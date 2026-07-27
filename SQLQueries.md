Get List of Suppliers:

```
SELECT
	[Code],
	[LastName],
	[FirstName]
FROM
	[Customers]
WHERE
	[CustType] = 'R' AND
	[InActive] = '0'
ORDER BY
	[LastName] Asc;
```

Get List of Branches:

```
SELECT
	[ID],
	[IsHO],
	[Name]
FROM
	[Branches]
WHERE
	[IsHO] = '0'
ORDER BY
	[Name] Asc;
```

Get Invoices:

```
SELECT
	[Branch],
	[SupplierCode],
	[InvoiceNumber],
	[Description],
	[InvoiceDate],
	[InvoiceAmount],
	[PONumber],
	[TaxAmount1],
	[Logged]
FROM
	[APInv]
WHERE
	[InvoiceDate] >= '' AND
	[InvoiceDate] < ''
ORDER BY
	[InvoiceDate] Desc;
```

Example dates for February (>= '2026-02-01 00:00:00.000') AND (< '2026-03-01 00:00:00.000')